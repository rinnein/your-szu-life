package `in`.rinne.your_szu_life

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationManagerCompat
import org.json.JSONArray

/**
 * 课表课前提醒的原生排程桥。
 *
 * Rust 侧只在课表/设置变化时调用 [sync]；Kotlin 负责把每节课拆成
 * T-20（发布常驻倒计时）→ 每 60 秒一次自续 tick（重算文案/进度）→ T0（收起）
 * → 下课（兜底清理）四类 AlarmManager 闹钟。通知由 [TimetableReminderReceiver]
 * 发布，Android 16+ 使用 ProgressStyle / Live Update，低版本回退到
 * setProgress + 系统倒计时。
 *
 * 实时活动只覆盖课前窗口：上课后不再发布任何提醒。
 */
object TimetableNotificationBridge {
  const val CHANNEL_ID = "class-reminder"
  const val ACTION = "in.rinne.your_szu_life.TIMETABLE_REMINDER"
  const val PREFS = "timetable_reminders"
  const val PREFS_SPECS = "specs"

  /** 通知与闹钟点击统一回到应用首页。 */
  const val HOME_URL = "your-szu-life://page/"

  @JvmStatic
  fun sync(context: Context, specsJson: String): Boolean = try {
    val specs = parseSpecs(specsJson)
    val app = context.applicationContext
    ensureChannel(app)

    val previous = loadSpecs(app)
    val nextIds = specs.map { it.sessionId }.toSet()
    previous.filter { it.sessionId !in nextIds }.forEach { cancelSpec(app, it) }

    specs.forEach { spec -> scheduleSpec(app, spec) }
    prefs(app).edit().putString(PREFS_SPECS, specsJson).apply()
    true
  } catch (_: Exception) {
    false
  }

  @JvmStatic
  fun cancelAll(context: Context): Boolean = try {
    val app = context.applicationContext
    TimetableLiveUpdateService.stop(app)
    loadSpecs(app).forEach { cancelSpec(app, it) }
    prefs(app).edit().remove(PREFS_SPECS).apply()
    true
  } catch (_: Exception) {
    false
  }

  /** 开机/时区变化/应用更新后从 SharedPreferences 恢复排程。 */
  fun restore(context: Context) {
    val raw = prefs(context.applicationContext).getString(PREFS_SPECS, null) ?: return
    sync(context, raw)
  }

  fun notificationId(sessionId: String): Int = sessionId.hashCode() and 0x3fff_ffff

  /** 把课次包进闹钟/服务 Intent（闹钟与实时活动共用同一组 extras）。 */
  fun putSpec(intent: Intent, spec: ClassSpec) {
    intent
      .putExtra(EXTRA_SESSION_ID, spec.sessionId)
      .putExtra(EXTRA_COURSE, spec.courseName)
      .putExtra(EXTRA_START, spec.startAtMs)
      .putExtra(EXTRA_END, spec.endAtMs)
      .putExtra(EXTRA_TIME_LABEL, spec.timeLabel)
      .putExtra(EXTRA_LOCATION, spec.location)
      .putExtra(EXTRA_LEAD, spec.leadMinutes)
      .putExtra(EXTRA_LIVE, spec.liveUpdate)
  }

  /** 从 Intent extras 还原课次；缺关键字段时返回 null。 */
  fun specFromIntent(intent: Intent?): ClassSpec? {
    if (intent == null) return null
    val sessionId = intent.getStringExtra(EXTRA_SESSION_ID) ?: return null
    val start = intent.getLongExtra(EXTRA_START, 0L)
    val end = intent.getLongExtra(EXTRA_END, 0L)
    if (start <= 0L || end <= start) return null
    return ClassSpec(
      sessionId = sessionId,
      courseName = intent.getStringExtra(EXTRA_COURSE).orEmpty(),
      startAtMs = start,
      endAtMs = end,
      timeLabel = intent.getStringExtra(EXTRA_TIME_LABEL).orEmpty(),
      location = intent.getStringExtra(EXTRA_LOCATION).orEmpty(),
      leadMinutes = intent.getLongExtra(EXTRA_LEAD, 20L).coerceAtLeast(1L),
      liveUpdate = intent.getBooleanExtra(EXTRA_LIVE, false),
    )
  }

  /**
   * 首页点击意图：显式指向 [MainActivity]，避免依赖深链 intent-filter 才能解析；
   * `CLEAR_TOP` 顺带收掉叠在主页面之上的独立页面窗口。
   */
  fun openHome(context: Context, sessionId: String): PendingIntent {
    val intent = Intent(context, MainActivity::class.java)
      .setAction(Intent.ACTION_VIEW)
      .setData(Uri.parse(HOME_URL))
      .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
    return PendingIntent.getActivity(
      context,
      requestCode(sessionId, POINT_HOME),
      intent,
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
  }

  /** 课前窗口内每分钟续一次，由接收器收到后再次调用。 */
  fun scheduleNextTick(context: Context, spec: ClassSpec, now: Long) {
    val delay = LiveReminderSession.nextAlarmDelay(now, spec.startAtMs)
    if (delay <= 0L) return
    val intent = Intent(context, TimetableReminderReceiver::class.java)
      .setAction(ACTION)
      .putExtra(EXTRA_PHASE, TimetableReminderReceiver.PHASE_TICK)
    putSpec(intent, spec)
    val pending = PendingIntent.getBroadcast(
      context,
      requestCode(spec.sessionId, POINT_TICK),
      intent,
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    setAlarm(context, now + delay, pending)
  }

  private fun scheduleSpec(context: Context, spec: ClassSpec) {
    val now = System.currentTimeMillis()
    val start = spec.startAtMs
    if (start <= now) return
    val leadMs = spec.leadMinutes.coerceAtLeast(1) * 60_000L
    val leadAt = start - leadMs
    if (leadAt <= now) {
      // 已经进入课前窗口：立刻启动实时活动或发一条定点通知，并接着跑 tick 链。
      TimetableReminderReceiver.refresh(context, spec)
    }
    schedule(context, spec, TimetableReminderReceiver.PHASE_REMIND, POINT_REMIND, leadAt)
    // 上课即收起：实时活动与定点通知都在这一刻结束。
    schedule(context, spec, TimetableReminderReceiver.PHASE_CLEAR, POINT_CLEAR, start)
    // 下课的兜底清理：错过 T0 闹钟时也不会把通知留到下一节。
    schedule(context, spec, TimetableReminderReceiver.PHASE_END, POINT_END, spec.endAtMs)
  }

  private fun schedule(
    context: Context,
    spec: ClassSpec,
    phase: Int,
    point: Int,
    atMs: Long,
  ) {
    if (atMs <= System.currentTimeMillis()) return
    val intent = Intent(context, TimetableReminderReceiver::class.java)
      .setAction(ACTION)
      .putExtra(EXTRA_PHASE, phase)
    putSpec(intent, spec)
    val pending = PendingIntent.getBroadcast(
      context,
      requestCode(spec.sessionId, point),
      intent,
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    setAlarm(context, atMs, pending)
  }

  private fun setAlarm(context: Context, atMs: Long, pending: PendingIntent) {
    val manager = context.getSystemService(AlarmManager::class.java)
    val exact = Build.VERSION.SDK_INT < 31 || manager.canScheduleExactAlarms()
    try {
      if (exact) manager.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMs, pending)
      else manager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMs, pending)
    } catch (_: SecurityException) {
      manager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMs, pending)
    }
  }

  private fun cancelSpec(context: Context, spec: ClassSpec) {
    val manager = context.getSystemService(AlarmManager::class.java)
    for (point in 0..POINT_END) {
      val intent = Intent(context, TimetableReminderReceiver::class.java).setAction(ACTION)
      val pending = PendingIntent.getBroadcast(
        context,
        requestCode(spec.sessionId, point),
        intent,
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
      )
      manager.cancel(pending)
      pending.cancel()
    }
    NotificationManagerCompat.from(context).cancel(notificationId(spec.sessionId))
    if (TimetableLiveUpdateBridge.castingSessionId() == spec.sessionId) {
      TimetableLiveUpdateService.stop(context)
    }
  }

  private fun requestCode(sessionId: String, point: Int): Int =
    (sessionId.hashCode() * 31 + point) and 0x7fff_ffff

  private fun ensureChannel(context: Context) {
    if (Build.VERSION.SDK_INT < 26) return
    val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    if (manager.getNotificationChannel(CHANNEL_ID) != null) return
    val channel = NotificationChannel(
      CHANNEL_ID,
      "上课提醒",
      NotificationManager.IMPORTANCE_DEFAULT,
    ).apply {
      description = "课前 20 分钟起显示常驻倒计时，上课后自动收起"
      setShowBadge(false)
    }
    manager.createNotificationChannel(channel)
  }

  private fun prefs(context: Context) =
    context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

  private fun loadSpecs(context: Context): List<ClassSpec> {
    val raw = prefs(context).getString(PREFS_SPECS, null) ?: return emptyList()
    return runCatching { parseSpecs(raw) }.getOrDefault(emptyList())
  }

  private fun parseSpecs(raw: String): List<ClassSpec> {
    if (raw.isBlank()) return emptyList()
    val array = JSONArray(raw)
    val specs = ArrayList<ClassSpec>(array.length())
    for (index in 0 until array.length()) {
      val row = array.getJSONObject(index)
      val sessionId = row.optString("sessionId")
      val course = row.optString("courseName")
      val start = row.optLong("startAtMs", 0L)
      val end = row.optLong("endAtMs", 0L)
      if (sessionId.isBlank() || start <= 0L || end <= start) continue
      specs.add(
        ClassSpec(
          sessionId = sessionId,
          courseName = course,
          startAtMs = start,
          endAtMs = end,
          timeLabel = row.optString("timeLabel"),
          location = row.optString("location"),
          leadMinutes = row.optLong("leadMinutes", 20L).coerceAtLeast(1L),
          liveUpdate = row.optBoolean("liveUpdate", false),
        )
      )
    }
    return specs
  }

  data class ClassSpec(
    val sessionId: String,
    val courseName: String,
    val startAtMs: Long,
    val endAtMs: Long,
    val timeLabel: String,
    val location: String,
    val leadMinutes: Long,
    /** 是否使用 Android 16 实时活动（promoted Live Update）推进展示。 */
    val liveUpdate: Boolean = false,
  )

  const val EXTRA_SESSION_ID = "sessionId"
  const val EXTRA_COURSE = "courseName"
  const val EXTRA_START = "startAtMs"
  const val EXTRA_END = "endAtMs"
  const val EXTRA_TIME_LABEL = "timeLabel"
  const val EXTRA_LOCATION = "location"
  const val EXTRA_LEAD = "leadMinutes"
  const val EXTRA_PHASE = "phase"
  const val EXTRA_LIVE = "liveUpdate"

  // 每个时间点必须有独立请求码；同 phase 共用请求码会让后一个闹钟覆盖前一个。
  private const val POINT_REMIND = 0
  private const val POINT_TICK = 1
  private const val POINT_CLEAR = 2
  private const val POINT_END = 3
  /** 点击意图与闹钟不是同一类 PendingIntent，给一个不冲突的请求码。 */
  private const val POINT_HOME = 4
}
