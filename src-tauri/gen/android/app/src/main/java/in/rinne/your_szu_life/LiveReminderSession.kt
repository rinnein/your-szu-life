package `in`.rinne.your_szu_life

/**
 * 实时活动（Android 16 promoted Live Update / ColorOS 流体云）的纯逻辑。
 *
 * 这里只做算术与文本，不碰 Android API：服务与接收器都调用这些函数，
 * 单测直接覆盖（与 `MobileViewportTest` / `WallpaperColorBridgeTest` 的约定一致）。
 */
object LiveReminderSession {
  /** 进度环与倒计时文字的重发间隔：promoted 卡片不会自己动，只能主动重发。 */
  const val TICK_MS: Long = 15_000L

  /** ColorOS 首次发帖存在竞态：稍后补发一次能稳定进入胶囊（sleepy 实测）。 */
  const val REINFORCE_DELAY_MS: Long = 600L

  /**
   * 进度百分比：课前窗口内从 0 线性走到 100，上课后固定 100。
   * 窗口起点 = `startAtMs - leadMinutes`，与通知文案里的倒计时同源。
   */
  fun progressPercent(startAtMs: Long, leadMinutes: Long, now: Long): Int {
    if (now >= startAtMs) return 100
    val leadMs = leadMinutes.coerceAtLeast(1L) * 60_000L
    val begin = startAtMs - leadMs
    if (now <= begin) return 0
    return (((now - begin) * 100) / leadMs).toInt().coerceIn(0, 100)
  }

  /** 通知正文与胶囊共用的一句话状态。 */
  fun statusText(startAtMs: Long, now: Long): String {
    if (now >= startAtMs) return "已开始"
    val remainingMinutes = ((startAtMs - now).coerceAtLeast(0L) / 60_000L).toInt()
    if (remainingMinutes <= 0) return "马上开始"
    return "还有 $remainingMinutes 分钟"
  }

  /**
   * 胶囊上的短文本。系统只显示极少字符，按 sleepy 的真机结论截到 7 个字符；
   * 课前的「还有 N 分钟」压缩成「N分」，上课后显示「上课中」，其它情况用课程名。
   */
  fun capsuleText(courseName: String, status: String): String {
    val text = when {
      status == "已开始" -> "上课中"
      status == "马上开始" -> "即将开始"
      status.startsWith("还有 ") && status.endsWith(" 分钟") ->
        status.removePrefix("还有 ").removeSuffix(" 分钟") + "分"
      else -> courseName
    }
    val trimmed = text.trim().ifBlank { courseName.trim() }
    return if (trimmed.length <= 7) trimmed else trimmed.take(7)
  }

  /** 下一次重发的延迟：不超过 tick，也不超过下课时间。 */
  fun nextTickDelay(now: Long, endAtMs: Long): Long {
    val remaining = endAtMs - now
    if (remaining <= 0L) return 0L
    return minOf(TICK_MS, remaining).coerceAtLeast(1L)
  }

  /**
   * 同一节课的重复启动请求（探测循环每轮都会同步）不应重建前台服务：
   * 只有课程或开始时间变了才需要换会话；已下课的情况交给停止流程。
   */
  fun canReuseSession(
    currentSessionId: String?,
    currentStartAtMs: Long,
    currentEndAtMs: Long,
    nextSessionId: String,
    nextStartAtMs: Long,
    nextEndAtMs: Long,
    casting: Boolean,
    now: Long,
  ): Boolean {
    if (!casting) return false
    if (currentSessionId != nextSessionId) return false
    if (currentStartAtMs != nextStartAtMs) return false
    if (currentEndAtMs != nextEndAtMs) return false
    return now < currentEndAtMs
  }

  /** ColorOS（OPPO / 一加 / realme）首帖需要补发一次，其它厂商不需要。 */
  fun needsFirstPostReinforcement(manufacturer: String): Boolean {
    val normalized = manufacturer.trim().lowercase()
    return normalized == "oppo" || normalized == "oneplus" || normalized == "realme"
  }
}
