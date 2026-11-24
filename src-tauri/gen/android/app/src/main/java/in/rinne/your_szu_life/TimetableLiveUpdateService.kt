package `in`.rinne.your_szu_life

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
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
 * 与一次次定点重发通知的区别：在课前窗口开始时启动前台服务，每 15 秒重发一次带
 * `ProgressStyle` 的通知来推进进度与小图标进度环，**到上课时间结束**。
 *
 * 这里只负责「进程活着时的顺滑路径」：每次发布都会写一条心跳，[TimetableReminderReceiver]
 * 的分钟级闹钟发现心跳过期时会自己重发并重启本服务。即使系统冻结/回收了进程，
 * 闹钟仍能唤醒应用把倒计时推进到下一分钟。
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
      if (now >= spec.startAtMs) {
        stopCasting()
        return
      }
      publish(spec)
      handler.postDelayed(this, LiveReminderSession.nextTickDelay(now, spec.startAtMs))
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
    if (now >= spec.startAtMs) {
      stopCasting()
      return START_NOT_STICKY
    }
    if (isReusable(spec, now)) {
      // 同一节课的重复续期：不重建通知（否则 vivo/ColorOS 的展开态会被重置），
      // 但立刻补发一次并重置循环——进程可能刚从冻结中恢复，旧的延迟已经不可信。
      handler.removeCallbacks(tick)
      handler.post(tick)
      return START_NOT_STICKY
    }
    session = spec
    casting = true
    lastSessionId = spec.sessionId
    TimetableLiveUpdateBridge.markCasting(true, spec.sessionId)
    ensureChannel()
    acquireWakeLock(spec)
    publish(spec)
    if (LiveReminderSession.needsFirstPostReinforcement(Build.MANUFACTURER.orEmpty())) {
      // ColorOS 首帖竞态：稍后再发一次同样的通知，胶囊才会稳定出现。
      handler.postDelayed({
        val current = session ?: return@postDelayed
        if (System.currentTimeMillis() < current.startAtMs) publish(current)
      }, LiveReminderSession.REINFORCE_DELAY_MS)
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
      nextSessionId = spec.sessionId,
      nextStartAtMs = spec.startAtMs,
      casting = casting,
      now = now,
    )
  }

  private fun publish(spec: TimetableNotificationBridge.ClassSpec) {
    val now = System.currentTimeMillis()
    val notification = buildLiveNotification(this, spec, now)
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
      TimetableLiveUpdateBridge.markPublished(spec.sessionId, now)
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
      description = "课前 20 分钟起以实时活动展示倒计时，上课后自动收起（Android 16 实时更新 / ColorOS 流体云）"
      setShowBadge(false)
    }
    manager.createNotificationChannel(channel)
  }

  /** 只在课前窗口内唤醒 CPU；固定 6 小时会白白耗电，也与窗口语义不符。 */
  private fun acquireWakeLock(spec: TimetableNotificationBridge.ClassSpec) {
    if (wakeLock?.isHeld == true) return
    val now = System.currentTimeMillis()
    val duration = (spec.startAtMs - now + 60_000L).coerceIn(60_000L, 30 * 60_000L)
    wakeLock = (getSystemService(Context.POWER_SERVICE) as PowerManager)
      .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "$packageName:live-class")
      .apply {
        setReferenceCounted(false)
        acquire(duration)
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
     *
     * 卡片文案固定为「标题 = 时间段 + 地点 / 表头 = 课程名 / 正文 = 倒计时」，
     * 右上角 logo 与强调色由 [NotificationBrand] 统一提供。
     */
    fun buildLiveNotification(
      context: Context,
      spec: TimetableNotificationBridge.ClassSpec,
      now: Long,
    ): Notification {
      val status = LiveReminderSession.statusText(spec.startAtMs, now)
      val texts = LiveReminderSession.notificationTexts(
        timeLabel = spec.timeLabel,
        location = spec.location,
        courseName = spec.courseName,
        startAtMs = spec.startAtMs,
        now = now,
      )
      val progress = LiveReminderSession.progressPercent(spec.startAtMs, spec.leadMinutes, now)
      val accent = NotificationBrand.accent(context)

      val builder = NotificationCompat.Builder(context, CHANNEL_ID)
        .setSmallIcon(
          ProgressRingIcon.create(progress)
            ?: IconCompat.createWithResource(context, R.drawable.ic_stat_timetable),
        )
        .setContentTitle(texts.title)
        .setContentText(texts.text)
        .setSubText(texts.subText)
        .setContentIntent(TimetableNotificationBridge.openHome(context, spec.sessionId))
        .setCategory(NotificationCompat.CATEGORY_PROGRESS)
        .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
        .setOngoing(true)
        .setAutoCancel(false)
        .setOnlyAlertOnce(true)
        .setSilent(true)
        .setProgress(100, progress, false)
        // 系统倒计时：指向上课时间。否则表头会显示每次重发的时刻。
        .setWhen(spec.startAtMs)
        .setUsesChronometer(true)
        .setChronometerCountDown(true)
        .setStyle(NotificationBrand.progressStyle(progress, accent))
      NotificationBrand.apply(builder, context, accent)

      if (supportsPromoted(context)) {
        builder.setRequestPromotedOngoing(true)
        builder.setShortCriticalText(LiveReminderSession.capsuleText(spec.courseName, status))
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
  }
}

/**
 * 从 Rust 桥接收实时活动请求（`TimedShowLive` 之外的启动路径由接收器直接调用
 * [TimetableLiveUpdateService.start]），并记录发布心跳供闹钟判断进程是否停摆。
 */
object TimetableLiveUpdateBridge {
  @Volatile
  private var castingSessionId: String? = null

  /** 最近一次成功发布通知的时间；0 表示当前进程还没有发布过。 */
  @Volatile
  private var publishedAtMs: Long = 0L

  fun markCasting(value: Boolean, sessionId: String?) {
    castingSessionId = if (value) sessionId else null
    if (!value) publishedAtMs = 0L
  }

  fun markPublished(sessionId: String, atMs: Long) {
    if (castingSessionId == sessionId) publishedAtMs = atMs
  }

  /** 当前是否正在投放实时活动（进程内状态；ColorOS 回前台会自动收起胶囊）。 */
  @JvmStatic
  fun isCasting(): Boolean = castingSessionId != null

  @JvmStatic
  fun castingSessionId(): String? = castingSessionId

  @JvmStatic
  fun lastPublishedAtMs(): Long = publishedAtMs
}
