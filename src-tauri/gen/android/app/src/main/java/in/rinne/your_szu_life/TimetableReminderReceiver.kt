package `in`.rinne.your_szu_life

import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat

/**
 * 课表课前提醒的发布/更新/清除入口。
 *
 * 通知内容固定包含课程名、上课时间段与地点；上课前显示倒计时。
 * - Android 16+ 且系统允许 promoted ongoing：ProgressStyle + Live Update；
 * - 其余版本：setProgress + setUsesChronometer(true) + setChronometerCountDown(true)。
 */
class TimetableReminderReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    if (intent.action != TimetableNotificationBridge.ACTION) {
      TimetableNotificationBridge.restore(context)
      return
    }
    val phase = intent.getIntExtra(TimetableNotificationBridge.EXTRA_PHASE, PHASE_REMIND)
    if (phase == PHASE_END) {
      val sessionId = intent.getStringExtra(TimetableNotificationBridge.EXTRA_SESSION_ID) ?: return
      if (TimetableLiveUpdateBridge.castingSessionId() == sessionId) {
        // 实时活动会话自己会在下课时收尾，这里只确保服务被停掉。
        TimetableLiveUpdateService.stop(context)
      }
      NotificationManagerCompat.from(context)
        .cancel(TimetableNotificationBridge.notificationId(sessionId))
      return
    }
    val spec = TimetableNotificationBridge.specFromIntent(intent) ?: return
    // 实时活动已接管该课次时，定点闹钟只做「启动/续期」与下课清理，不再重复发帖，
    // 否则会和前台服务每 15 秒的通知互相覆盖。
    if (spec.liveUpdate && TimetableLiveUpdateService.supportsPromoted(context)) {
      if (phase == PHASE_REMIND || phase == PHASE_START) {
        TimetableLiveUpdateService.start(context, spec)
      }
      if (TimetableLiveUpdateBridge.castingSessionId() == spec.sessionId) return
    }
    post(context, spec, phase)
  }

  companion object {
    const val PHASE_REMIND = 0
    const val PHASE_UPDATE = 1
    const val PHASE_START = 2
    const val PHASE_END = 3

    fun post(context: Context, spec: TimetableNotificationBridge.ClassSpec, phase: Int) {
      if (Build.VERSION.SDK_INT >= 33) {
        val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (!manager.areNotificationsEnabled()) return
      }
      val app = context.applicationContext
      val now = System.currentTimeMillis()
      val started = phase == PHASE_START || now >= spec.startAtMs
      val remainingMinutes = ((spec.startAtMs - now).coerceAtLeast(0L) / 60_000L).toInt()
      val base = listOf(spec.timeLabel, spec.location)
        .filter { it.isNotBlank() }
        .joinToString(" · ")
      val status = if (started) "已开始" else "还有 $remainingMinutes 分钟"
      val text = listOf(base, status)
        .filter { it.isNotBlank() }
        .joinToString(" · ")
      val progress = progressPercent(spec, now)
      val open = openIntent(app, spec)
      val notificationId = TimetableNotificationBridge.notificationId(spec.sessionId)

      val builder = NotificationCompat.Builder(app, TimetableNotificationBridge.CHANNEL_ID)
        .setSmallIcon(R.drawable.ic_stat_timetable)
        .setContentTitle(spec.courseName)
        .setContentText(text)
        .setStyle(NotificationCompat.BigTextStyle().bigText(text))
        .setContentIntent(open)
        .setOngoing(now < spec.endAtMs)
        .setOnlyAlertOnce(true)
        .setAutoCancel(false)
        .setCategory(NotificationCompat.CATEGORY_PROGRESS)
        .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
        .setProgress(100, progress, false)
        .setWhen(if (started) spec.endAtMs else spec.startAtMs)
        .setUsesChronometer(true)
        .setChronometerCountDown(true)

      if (supportsPromoted(app)) {
        // 非实时活动路径下也尽量带上 promoted 三件套（系统自行决定是否提升）；
        // 有前台服务时这条会被服务的每 15s 发帖覆盖，因此只是兜底。
        builder.setStyle(NotificationCompat.ProgressStyle().setProgress(progress))
        builder.setRequestPromotedOngoing(true)
        builder.setShortCriticalText(LiveReminderSession.capsuleText(spec.courseName, status))
      }

      try {
        NotificationManagerCompat.from(app).notify(notificationId, builder.build())
      } catch (_: SecurityException) {
        // 没有通知权限时静默降级；应用内课表仍然可用。
      }
    }

    /** 进度 = 已进入 20 分钟窗口的比例；上课后为 100。 */
    fun progressPercent(
      spec: TimetableNotificationBridge.ClassSpec,
      now: Long,
    ): Int {
      if (now >= spec.startAtMs) return 100
      val leadMs = spec.leadMinutes.coerceAtLeast(1L) * 60_000L
      val begin = spec.startAtMs - leadMs
      if (now <= begin) return 0
      return (((now - begin) * 100) / leadMs).toInt().coerceIn(0, 100)
    }

    private fun supportsPromoted(context: Context): Boolean {
      if (Build.VERSION.SDK_INT < 36) return false
      val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
      return manager.canPostPromotedNotifications()
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
