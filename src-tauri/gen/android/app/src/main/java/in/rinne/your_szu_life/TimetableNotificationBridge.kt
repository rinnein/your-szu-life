package `in`.rinne.your_szu_life

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationManagerCompat
import org.json.JSONArray
import org.json.JSONObject

/**
 * 课表课前提醒的原生排程桥。
 *
 * Rust 侧只在课表/设置变化时调用 [sync]；Kotlin 负责把每节课拆成
 * T-20（发布常驻倒计时）、T-10/T-5（更新进度）、T0（切换为已开始）、
 * 下课（清除）五个 AlarmManager 闹钟。通知由 [TimetableReminderReceiver]
 * 发布，Android 16+ 使用 ProgressStyle / Live Update，低版本回退到
 * setProgress + 系统倒计时。
 */
object TimetableNotificationBridge {
  const val CHANNEL_ID = "class-reminder"
  const val ACTION = "in.rinne.your_szu_life.TIMETABLE_REMINDER"
  const val PREFS = "timetable_reminders"
  const val PREFS_SPECS = "specs"

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
    loadSpecs(app).forEach { cancelSpec(app, it) }
    prefs(app).edit().remove(PREFS_SPECS).apply()
    true
  } catch (_: Exception) {
    false
  }

  /** 开机/时区变化后从 SharedPreferences 恢复排程。 */
  fun restore(context: Context) {
    val raw = prefs(context.applicationContext).getString(PREFS_SPECS, null) ?: return
    sync(context, raw)
  }

  fun notificationId(sessionId: String): Int = sessionId.hashCode() and 0x3fff_ffff

  private fun scheduleSpec(context: Context, spec: ClassSpec) {
    val now = System.currentTimeMillis()
    val start = spec.startAtMs
    if (start <= now) return
    val leadMs = spec.leadMinutes.coerceAtLeast(1) * 60_000L
    val leadAt = start - leadMs
    if (leadAt <= now) {
      // 已经进入 20 分钟窗口：立即发布，后续 T0/下课仍由闹钟推进。
      TimetableReminderReceiver.post(context, spec, TimetableReminderReceiver.PHASE_REMIND)
    }
    schedule(context, spec, TimetableReminderReceiver.PHASE_REMIND, POINT_REMIND, leadAt)
    schedule(context, spec, TimetableReminderReceiver.PHASE_UPDATE, POINT_UPDATE_15, start - 15 * 60_000L)
    schedule(context, spec, TimetableReminderReceiver.PHASE_UPDATE, POINT_UPDATE_10, start - 10 * 60_000L)
    schedule(context, spec, TimetableReminderReceiver.PHASE_UPDATE, POINT_UPDATE_5, start - 5 * 60_000L)
    schedule(context, spec, TimetableReminderReceiver.PHASE_START, POINT_START, start)
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
      .putExtra(EXTRA_SESSION_ID, spec.sessionId)
      .putExtra(EXTRA_COURSE, spec.courseName)
      .putExtra(EXTRA_START, spec.startAtMs)
      .putExtra(EXTRA_END, spec.endAtMs)
      .putExtra(EXTRA_TIME_LABEL, spec.timeLabel)
      .putExtra(EXTRA_LOCATION, spec.location)
      .putExtra(EXTRA_ROUTE, spec.route)
      .putExtra(EXTRA_LEAD, spec.leadMinutes)
      .putExtra(EXTRA_PHASE, phase)
    val pending = PendingIntent.getBroadcast(
      context,
      requestCode(spec.sessionId, point),
      intent,
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
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
      description = "课前 20 分钟起显示常驻倒计时，并在上课时更新"
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
          route = row.optString("route"),
          leadMinutes = row.optLong("leadMinutes", 20L).coerceAtLeast(1L),
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
    val route: String,
    val leadMinutes: Long,
  )

  const val EXTRA_SESSION_ID = "sessionId"
  const val EXTRA_COURSE = "courseName"
  const val EXTRA_START = "startAtMs"
  const val EXTRA_END = "endAtMs"
  const val EXTRA_TIME_LABEL = "timeLabel"
  const val EXTRA_LOCATION = "location"
  const val EXTRA_ROUTE = "route"
  const val EXTRA_LEAD = "leadMinutes"
  const val EXTRA_PHASE = "phase"

  // 每个时间点必须有独立请求码；同 phase 共用请求码会让后一个闹钟覆盖前一个。
  private const val POINT_REMIND = 0
  private const val POINT_UPDATE_15 = 1
  private const val POINT_UPDATE_10 = 2
  private const val POINT_UPDATE_5 = 3
  private const val POINT_START = 4
  private const val POINT_END = 5
}
