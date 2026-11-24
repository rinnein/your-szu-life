package `in`.rinne.your_szu_life

/**
 * 实时活动（Android 16 promoted Live Update / ColorOS 流体云）的纯逻辑。
 *
 * 实时活动只覆盖课前窗口 `[startAtMs - lead, startAtMs)`：上课铃一响就收起，
 * 不再显示「上课中」。窗口内的进度、文案与重发节奏都由这里产出，服务与接收器
 * 共用同一套函数，单测直接覆盖（与 `MobileViewportTest` / `WallpaperColorBridgeTest` 的约定一致）。
 */
object LiveReminderSession {
  /** 服务存活时推进进度环与倒计时的重发间隔。 */
  const val TICK_MS: Long = 15_000L

  /**
   * 后台兜底闹钟的重发间隔。闹钟由 AlarmManager 投递，进程被冻结/回收后仍会
   * 唤醒应用重发通知；进程内 15 秒循环只是顺滑路径，不能当作唯一数据源。
   */
  const val ALARM_TICK_MS: Long = 60_000L

  /** 发布心跳超过这个时长就认为服务已停摆，接收器要自己重发并重启服务。 */
  const val STALE_MS: Long = 45_000L

  /** ColorOS 首次发帖存在竞态：稍后补发一次能稳定进入胶囊（sleepy 实测）。 */
  const val REINFORCE_DELAY_MS: Long = 600L

  /**
   * 距上课还有几分钟：向上取整，最后一分钟内仍返回 1，避免出现「0 分钟后上课」。
   * 胶囊与正文共用这一份口径，不再一个 floor 一个 ceil。
   */
  fun remainingMinutes(startAtMs: Long, now: Long): Int {
    val remaining = startAtMs - now
    if (remaining <= 0L) return 0
    return ((remaining + 59_999L) / 60_000L).toInt()
  }

  /**
   * 进度百分比：课前窗口内从 0 线性走到 100。
   * 窗口起点 = `startAtMs - leadMinutes`，与通知文案里的倒计时同源。
   */
  fun progressPercent(startAtMs: Long, leadMinutes: Long, now: Long): Int {
    if (now >= startAtMs) return 100
    val leadMs = leadMinutes.coerceAtLeast(1L) * 60_000L
    val begin = startAtMs - leadMs
    if (now <= begin) return 0
    return (((now - begin) * 100) / leadMs).toInt().coerceIn(0, 100)
  }

  /** 通知正文与胶囊共用的一句话状态；只在课前窗口内调用，到点后的值是防御性回退。 */
  fun statusText(startAtMs: Long, now: Long): String {
    val minutes = remainingMinutes(startAtMs, now)
    return if (minutes <= 0) "即将开始" else "还有 $minutes 分钟"
  }

  /** 通知卡片的三个文字槽位：标题、表头小字与正文。 */
  data class ClassNotificationTexts(
    val title: String,
    val subText: String,
    val text: String,
  )

  /**
   * 实时活动/提醒卡片的文案（两条发布路径共用，避免文案分叉）：
   * - 标题：`上课时间段 上课地点`（没有地点时只留时间段）；
   * - 表头小字：课程名；
   * - 正文：`N 分钟后上课`，按分钟向上取整，最后一分钟内仍显示 1。
   *
   * 上课后不再发布通知，到点后的分支只是防御性回退。
   */
  fun notificationTexts(
    timeLabel: String,
    location: String,
    courseName: String,
    startAtMs: Long,
    now: Long,
  ): ClassNotificationTexts {
    val title = listOf(timeLabel, location)
      .map { it.trim() }
      .filter { it.isNotEmpty() }
      .joinToString(" ")
    val minutes = remainingMinutes(startAtMs, now)
    return ClassNotificationTexts(
      title = title,
      subText = courseName.trim(),
      text = if (minutes <= 0) "即将开始" else "$minutes 分钟后上课",
    )
  }

  /**
   * 胶囊上的短文本。系统只显示极少字符，按 sleepy 的真机结论截到 7 个字符；
   * 课前的「还有 N 分钟」压缩成「N分」，其它情况用课程名。
   */
  fun capsuleText(courseName: String, status: String): String {
    val text = when {
      status == "即将开始" -> "即将开始"
      status.startsWith("还有 ") && status.endsWith(" 分钟") ->
        status.removePrefix("还有 ").removeSuffix(" 分钟") + "分"
      else -> courseName
    }
    val trimmed = text.trim().ifBlank { courseName.trim() }
    return if (trimmed.length <= 7) trimmed else trimmed.take(7)
  }

  /** 服务内循环的下一次重发延迟：不超过 tick，也不越过上课时间。 */
  fun nextTickDelay(now: Long, startAtMs: Long): Long {
    val remaining = startAtMs - now
    if (remaining <= 0L) return 0L
    return minOf(TICK_MS, remaining).coerceAtLeast(1L)
  }

  /** 闹钟链的下一次唤醒延迟：不超过一分钟，也不越过上课时间。 */
  fun nextAlarmDelay(now: Long, startAtMs: Long): Long {
    val remaining = startAtMs - now
    if (remaining <= 0L) return 0L
    return minOf(ALARM_TICK_MS, remaining).coerceAtLeast(1L)
  }

  /**
   * 进程内的实时活动循环是否已停摆：没在投放、或发布心跳过期，都说明
   * 15 秒循环没有在跑（进程被冻结/回收），接收器必须自己重发并重启服务。
   */
  fun liveUpdateStale(casting: Boolean, lastPublishedAtMs: Long, now: Long): Boolean {
    if (!casting) return true
    if (lastPublishedAtMs <= 0L) return true
    return now - lastPublishedAtMs >= STALE_MS
  }

  /**
   * 同一节课的重复启动请求（闹钟链每一分钟都会续期）不应重建前台服务：
   * 只有课程或开始时间变了才需要换会话；已到上课时间的情况交给停止流程。
   */
  fun canReuseSession(
    currentSessionId: String?,
    currentStartAtMs: Long,
    nextSessionId: String,
    nextStartAtMs: Long,
    casting: Boolean,
    now: Long,
  ): Boolean {
    if (!casting) return false
    if (currentSessionId != nextSessionId) return false
    if (currentStartAtMs != nextStartAtMs) return false
    return now < currentStartAtMs
  }

  /** ColorOS（OPPO / 一加 / realme）首帖需要补发一次，其它厂商不需要。 */
  fun needsFirstPostReinforcement(manufacturer: String): Boolean {
    val normalized = manufacturer.trim().lowercase()
    return normalized == "oppo" || normalized == "oneplus" || normalized == "realme"
  }
}
