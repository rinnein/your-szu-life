package `in`.rinne.your_szu_life

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.graphics.drawable.IconCompat

/**
 * 上课提醒的实时活动通道（Android 16 promoted Live Update；ColorOS 流体云据此渲染）。
 *
 * 与一次次定点重发通知的区别：在课前 20 分钟窗口开始时启动前台服务，每 15 秒
 * 重发一次带 `ProgressStyle` 的通知来推进进度与小图标进度环，上课时切成「已开始」，
 * 下课结束。promoted 卡片不会自己动，这个循环是唯一能让进度前进的办法。
 *
 * 是否使用本通道由 `TimedShowLive` 决定（来自 Rust 侧的 `ClassReminderSpec.liveUpdate`），
 * 关闭或系统不支持时完全退回 [TimetableReminderReceiver] 的定点通知。
 */
class TimetableLiveUpdateService : Service() {
  private val handler = Handler(Looper.getMainLooper())
  private var session: TimetableNotificationBridge.ClassSpec? = null
  private var casting = false
  /** 记住要撤销的通知 id：stopForeground 要求先有一条通知在手。 */
  private var lastSessionId: String? = null
  private var wakeLock: PowerManager.WakeLock? = null

  private val tick = object : Runnable {
    override fun run() {
      val spec = session ?: return
      val now = System.currentTimeMillis()
      if (now >= spec.endAtMs) {
        stopCasting()
        return
      }
      publish(spec)
      handler.postDelayed(this, LiveReminderSession.nextTickDelay(now, spec.endAtMs))
    }
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == ACTION_STOP) {
      stopCasting()
      return START_NOT_STICKY
    }
    val spec = TimetableNotificationBridge.specFromIntent(intent) ?: run {
      stopCasting()
      return START_NOT_STICKY
    }
    val now = System.currentTimeMillis()
    if (now >= spec.endAtMs) {
      stopCasting()
      return START_NOT_STICKY
    }
    if (isReusable(spec, now)) {
      // 同一节课的重复同步：只继续循环，不重建通知（否则 vivo/ColorOS 的展开态会被重置）。
      return START_NOT_STICKY
    }
    session = spec
    casting = true
    lastSessionId = spec.sessionId
    TimetableLiveUpdateBridge.markCasting(true, spec.sessionId)
    ensureChannel()
    acquireWakeLock()
    publish(spec)
    if (LiveReminderSession.needsFirstPostReinforcement(Build.MANUFACTURER.orEmpty())) {
      // ColorOS 首帖竞态：稍后再发一次同样的通知，胶囊才会稳定出现。
      handler.postDelayed({ session?.let(::publish) }, LiveReminderSession.REINFORCE_DELAY_MS)
    }
    handler.removeCallbacks(tick)
    handler.postDelayed(tick, LiveReminderSession.TICK_MS)
    return START_NOT_STICKY
  }

  private fun isReusable(spec: TimetableNotificationBridge.ClassSpec, now: Long): Boolean {
    val current = session ?: return false
    return LiveReminderSession.canReuseSession(
      currentSessionId = current.sessionId,
      currentStartAtMs = current.startAtMs,
      currentEndAtMs = current.endAtMs,
      nextSessionId = spec.sessionId,
      nextStartAtMs = spec.startAtMs,
      nextEndAtMs = spec.endAtMs,
      casting = casting,
      now = now,
    )
  }

  private fun publish(spec: TimetableNotificationBridge.ClassSpec) {
    val notification = buildLiveNotification(this, spec, System.currentTimeMillis())
    try {
      if (Build.VERSION.SDK_INT >= 29) {
        startForeground(
          TimetableNotificationBridge.notificationId(spec.sessionId),
          notification,
          ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
        )
      } else {
        startForeground(TimetableNotificationBridge.notificationId(spec.sessionId), notification)
      }
    } catch (_: Throwable) {
      // 前台服务被系统拒绝（例如 FGS 配额）：退回定点通知，不中断提醒。
      stopCasting()
    }
  }

  private fun stopCasting() {
    handler.removeCallbacks(tick)
    session = null
    casting = false
    TimetableLiveUpdateBridge.markCasting(false, null)
    releaseWakeLock()
    if (Build.VERSION.SDK_INT >= 26) {
      // 结束前先给一条低优先级占位通知，满足「stopForeground 前必须有通知」的契约。
      try {
        val placeholder = NotificationCompat.Builder(this, CHANNEL_ID)
          .setSmallIcon(R.drawable.ic_stat_timetable)
          .setContentTitle(getString(R.string.app_name))
          .setPriority(NotificationCompat.PRIORITY_LOW)
          .build()
        startForeground(FALLBACK_ID, placeholder)
      } catch (_: Throwable) {
        // 已经不在前台时忽略。
      }
    }
    // 只撤掉本课程的实时活动通知（占位通知走 STOP_FOREGROUND_REMOVE）。
    lastSessionId?.let { NotificationManagerCompat.from(this).cancel(TimetableNotificationBridge.notificationId(it)) }
    stopForeground(STOP_FOREGROUND_REMOVE)
    stopSelf()
  }

  override fun onTimeout(startId: Int, fgsType: Int) {
    stopCasting()
  }

  override fun onDestroy() {
    handler.removeCallbacks(tick)
    casting = false
    TimetableLiveUpdateBridge.markCasting(false, null)
    releaseWakeLock()
    super.onDestroy()
  }

  private fun ensureChannel() {
    if (Build.VERSION.SDK_INT < 26) return
    val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    if (manager.getNotificationChannel(CHANNEL_ID) != null) return
    // MIN 重要度会失去 promoted 资格，必须是 DEFAULT 及以上。
    val channel = NotificationChannel(CHANNEL_ID, "上课实时活动", NotificationManager.IMPORTANCE_HIGH).apply {
      description = "课前 20 分钟起以实时活动展示倒计时（Android 16 实时更新 / ColorOS 流体云）"
      setShowBadge(false)
    }
    manager.createNotificationChannel(channel)
  }

  private fun acquireWakeLock() {
    if (wakeLock?.isHeld == true) return
    wakeLock = (getSystemService(Context.POWER_SERVICE) as PowerManager)
      .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "$packageName:live-class")
      .apply {
        setReferenceCounted(false)
        acquire(6 * 60 * 60 * 1000L)
      }
  }

  private fun releaseWakeLock() {
    wakeLock?.let { if (it.isHeld) it.release() }
    wakeLock = null
  }

  companion object {
    const val CHANNEL_ID = "class-live"
    const val ACTION_START = "in.rinne.your_szu_life.LIVE_CLASS_START"
    const val ACTION_STOP = "in.rinne.your_szu_life.LIVE_CLASS_STOP"
    private const val FALLBACK_ID = 0x4c495645 // "LIVE"

    /** 启动（或续期）实时活动。失败时返回 false，调用方回退定点通知。 */
    fun start(context: Context, spec: TimetableNotificationBridge.ClassSpec): Boolean = try {
      val app = context.applicationContext
      val intent = Intent(app, TimetableLiveUpdateService::class.java)
        .setAction(ACTION_START)
        .also { TimetableNotificationBridge.putSpec(it, spec) }
      if (Build.VERSION.SDK_INT >= 26) app.startForegroundService(intent) else app.startService(intent)
      true
    } catch (_: Exception) {
      false
    }

    fun stop(context: Context) {
      try {
        val app = context.applicationContext
        app.stopService(Intent(app, TimetableLiveUpdateService::class.java))
      } catch (_: Exception) {
        // 服务未运行时忽略。
      }
      TimetableLiveUpdateBridge.markCasting(false, null)
    }

    /**
     * 构建实时活动通知。ColorOS 流体云读取的是标准 promoted 通知：
     * `ProgressStyle` + `setRequestPromotedOngoing` + `setShortCriticalText`，
     * 因此不要用 RemoteViews、不要 group summary、不要 colorized。
     */
    fun buildLiveNotification(
      context: Context,
      spec: TimetableNotificationBridge.ClassSpec,
      now: Long,
    ): Notification {
      val started = now >= spec.startAtMs
      val status = LiveReminderSession.statusText(spec.startAtMs, now)
      val progress = LiveReminderSession.progressPercent(spec.startAtMs, spec.leadMinutes, now)
      val detail = listOf(spec.timeLabel, spec.location)
        .filter { it.isNotBlank() }
        .joinToString(" · ")

      val builder = NotificationCompat.Builder(context, CHANNEL_ID)
        .setSmallIcon(
          ProgressRingIcon.create(progress)
            ?: IconCompat.createWithResource(context, R.drawable.ic_stat_timetable),
        )
        .setContentTitle(spec.courseName)
        .setContentText(detail)
        .setSubText(status)
        .setContentIntent(openIntent(context, spec))
        .setCategory(NotificationCompat.CATEGORY_PROGRESS)
        .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
        .setOngoing(true)
        .setAutoCancel(false)
        .setOnlyAlertOnce(true)
        .setSilent(true)
        .setProgress(100, progress, false)
        .setStyle(NotificationCompat.ProgressStyle().setProgress(progress))

      if (supportsPromoted(context)) {
        builder.setRequestPromotedOngoing(true)
        builder.setShortCriticalText(LiveReminderSession.capsuleText(spec.courseName, status))
      }
      if (!started) {
        // 课前保留系统倒计时（点开通知后可见），promoted 卡片用它做辅助说明。
        builder.setWhen(spec.startAtMs)
          .setUsesChronometer(true)
          .setChronometerCountDown(true)
      }
      return builder.build()
    }

    fun supportsPromoted(context: Context): Boolean {
      if (Build.VERSION.SDK_INT < 36) return false
      return try {
        val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        manager.canPostPromotedNotifications()
      } catch (_: Throwable) {
        false
      }
    }

    private fun openIntent(
      context: Context,
      spec: TimetableNotificationBridge.ClassSpec,
    ): PendingIntent {
      val route = spec.route.ifBlank { "/study/timetable" }
      val path = if (route.startsWith("/")) route else "/$route"
      val uri = Uri.parse("your-szu-life://page$path")
      val intent = Intent(Intent.ACTION_VIEW, uri)
        .setPackage(context.packageName)
        .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
      return PendingIntent.getActivity(
        context,
        TimetableNotificationBridge.notificationId(spec.sessionId),
        intent,
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
      )
    }
  }
}

/**
 * 从 Rust 桥接收实时活动请求（`TimedShowLive` 之外的启动路径由接收器直接调用
 * [TimetableLiveUpdateService.start]）。
 */
object TimetableLiveUpdateBridge {
  @Volatile
  private var castingSessionId: String? = null

  fun markCasting(value: Boolean, sessionId: String?) {
    castingSessionId = if (value) sessionId else null
  }

  /** 当前是否正在投放实时活动（进程内状态；ColorOS 回前台会自动收起胶囊）。 */
  @JvmStatic
  fun isCasting(): Boolean = castingSessionId != null

  @JvmStatic
  fun castingSessionId(): String? = castingSessionId
}
