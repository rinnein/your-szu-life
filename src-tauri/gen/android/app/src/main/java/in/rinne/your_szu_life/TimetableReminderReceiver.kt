package `in`.rinne.your_szu_life

import android.app.NotificationManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat

/**
 * 课表课前提醒的发布/更新/清除入口。
 *
 * 文案固定为「标题 = 时间段 + 地点 / 表头小字 = 课程名 / 正文 = N 分钟后上课」，
 * 与实时活动共用 [LiveReminderSession.notificationTexts]；logo 与强调色共用 [NotificationBrand]。
 * - Android 16+ 且系统允许 promoted ongoing：ProgressStyle + Live Update；
 * - 其余版本：setProgress + setUsesChronometer(true) + setChronometerCountDown(true)。
 *
 * 刷新由 AlarmManager 的每分钟 tick 驱动（不依赖进程存活）；前台服务只是顺滑路径，
 * 一旦发布心跳过期就由本接收器重发并重启服务。上课后不再发布，只做清理。
 */
class TimetableReminderReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    if (intent.action != TimetableNotificationBridge.ACTION) {
      TimetableNotificationBridge.restore(context)
      return
    }
    val spec = TimetableNotificationBridge.specFromIntent(intent) ?: return
    val phase = intent.getIntExtra(TimetableNotificationBridge.EXTRA_PHASE, PHASE_REMIND)
    if (phase == PHASE_CLEAR || phase == PHASE_END) {
      clear(context, spec)
      return
    }
    refresh(context, spec)
  }

  companion object {
    const val PHASE_REMIND = 0
    const val PHASE_TICK = 1
    /** 上课时刻：收起通知，不再显示「上课中」。 */
    const val PHASE_CLEAR = 2
    /** 下课时刻的兜底清理（错过 T0 闹钟时兜住）。 */
    const val PHASE_END = 3

    /**
     * 课前窗口内的一次刷新。
     *
     * 优先让实时活动服务自己推进；若它没在投放、或发布心跳已过期（进程被冻结/
     * 回收），就在这里直接重发一条按当前时间重算的通知，并重启服务。
     * 每次刷新都会续上下一分钟的 tick。
     */
    fun refresh(context: Context, spec: TimetableNotificationBridge.ClassSpec) {
      val app = context.applicationContext
      val now = System.currentTimeMillis()
      if (now >= spec.startAtMs) {
        clear(app, spec)
        return
      }
      val live = spec.liveUpdate && TimetableLiveUpdateService.supportsPromoted(app)
      val stale = LiveReminderSession.liveUpdateStale(
        casting = TimetableLiveUpdateBridge.isCasting(),
        lastPublishedAtMs = TimetableLiveUpdateBridge.lastPublishedAtMs(),
        now = now,
      )
      if (live && stale) {
        // 闹钟把被冻结的进程唤醒：重启/续期前台服务，让它立刻补一次。
        TimetableLiveUpdateService.start(app, spec)
      }
      if (!live || stale) {
        post(app, spec)
      }
      TimetableNotificationBridge.scheduleNextTick(app, spec, now)
    }

    /** 停止实时活动并撤销通知（上课时刻与下课兜底共用）。 */
    fun clear(context: Context, spec: TimetableNotificationBridge.ClassSpec) {
      val app = context.applicationContext
      if (TimetableLiveUpdateBridge.castingSessionId() == spec.sessionId) {
        TimetableLiveUpdateService.stop(app)
      }
      NotificationManagerCompat.from(app)
        .cancel(TimetableNotificationBridge.notificationId(spec.sessionId))
    }

    private fun post(context: Context, spec: TimetableNotificationBridge.ClassSpec) {
      val app = context.applicationContext
      val now = System.currentTimeMillis()
      val notificationId = TimetableNotificationBridge.notificationId(spec.sessionId)
      if (now >= spec.startAtMs) {
        NotificationManagerCompat.from(app).cancel(notificationId)
        return
      }
      if (Build.VERSION.SDK_INT >= 33) {
        val manager = app.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (!manager.areNotificationsEnabled()) return
      }
      val status = LiveReminderSession.statusText(spec.startAtMs, now)
      val texts = LiveReminderSession.notificationTexts(
        timeLabel = spec.timeLabel,
        location = spec.location,
        courseName = spec.courseName,
        startAtMs = spec.startAtMs,
        now = now,
      )
      val progress = LiveReminderSession.progressPercent(spec.startAtMs, spec.leadMinutes, now)
      val accent = NotificationBrand.accent(app)

      val builder = NotificationCompat.Builder(app, TimetableNotificationBridge.CHANNEL_ID)
        .setSmallIcon(R.drawable.ic_stat_timetable)
        .setContentTitle(texts.title)
        .setContentText(texts.text)
        .setSubText(texts.subText)
        .setStyle(NotificationCompat.BigTextStyle().bigText(texts.text))
        .setContentIntent(TimetableNotificationBridge.openHome(app, spec.sessionId))
        .setOngoing(true)
        .setOnlyAlertOnce(true)
        .setAutoCancel(false)
        .setCategory(NotificationCompat.CATEGORY_PROGRESS)
        .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
        .setProgress(100, progress, false)
        // 系统倒计时由 SystemUI 自己走，进程被冻结时它仍然在动。
        .setWhen(spec.startAtMs)
        .setUsesChronometer(true)
        .setChronometerCountDown(true)
      NotificationBrand.apply(builder, app, accent)

      if (TimetableLiveUpdateService.supportsPromoted(app)) {
        // 非实时活动路径下也尽量带上 promoted 三件套（系统自行决定是否提升）；
        // 有前台服务时这条会被服务的 15 秒发帖覆盖，因此只是兜底。
        builder.setStyle(NotificationBrand.progressStyle(progress, accent))
        builder.setRequestPromotedOngoing(true)
        builder.setShortCriticalText(LiveReminderSession.capsuleText(spec.courseName, status))
      }

      try {
        NotificationManagerCompat.from(app).notify(notificationId, builder.build())
      } catch (_: SecurityException) {
        // 没有通知权限时静默降级；应用内课表仍然可用。
      }
    }
  }
}
