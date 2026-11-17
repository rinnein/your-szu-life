package `in`.rinne.your_szu_life

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** 实时活动纯逻辑：进度、胶囊文本、重发节奏与 ColorOS 首帖补发判断。 */
class LiveReminderSessionTest {
  private val minute = 60_000L
  private val base = 1_800_000_000_000L

  @Test
  fun progressMovesFromZeroToHundredAcrossTheLeadWindow() {
    assertEquals(0, LiveReminderSession.progressPercent(base, 20, base - 20 * minute))
    assertEquals(50, LiveReminderSession.progressPercent(base, 20, base - 10 * minute))
    assertEquals(100, LiveReminderSession.progressPercent(base, 20, base))
    // 上课后不再变化。
    assertEquals(100, LiveReminderSession.progressPercent(base, 20, base + 30 * minute))
    // 窗口之外夹紧到 0。
    assertEquals(0, LiveReminderSession.progressPercent(base, 20, base - 40 * minute))
  }

  @Test
  fun statusTextCountsDownThenSaysStarted() {
    assertEquals("还有 20 分钟", LiveReminderSession.statusText(base, base - 20 * minute))
    assertEquals("还有 1 分钟", LiveReminderSession.statusText(base, base - minute))
    assertEquals("马上开始", LiveReminderSession.statusText(base, base - 1_000L))
    assertEquals("已开始", LiveReminderSession.statusText(base, base + minute))
  }

  @Test
  fun capsuleTextPrefersCountdownThenClockIn() {
    assertEquals("14分", LiveReminderSession.capsuleText("概率论", "还有 14 分钟"))
    assertEquals("上课中", LiveReminderSession.capsuleText("概率论", "已开始"))
    assertEquals("即将开始", LiveReminderSession.capsuleText("概率论", "马上开始"))
    // 非倒计时状态回退课程名，并截到 7 个字符（胶囊显示宽度限制）。
    assertEquals("概率论与数理统", LiveReminderSession.capsuleText("概率论与数理统计", "其它"))
    assertEquals("", LiveReminderSession.capsuleText("   ", "其它"))
  }

  @Test
  fun tickDelayNeverExceedsFifteenSecondsOrTheEnd() {
    assertEquals(LiveReminderSession.TICK_MS, LiveReminderSession.nextTickDelay(base, base + 10 * minute))
    assertEquals(5_000L, LiveReminderSession.nextTickDelay(base, base + 5_000L))
    assertEquals(0L, LiveReminderSession.nextTickDelay(base, base))
  }

  @Test
  fun sessionIsReusedOnlyForTheSameClassStillRunning() {
    assertTrue(
      LiveReminderSession.canReuseSession(
        currentSessionId = "a|2026-11-17|1000",
        currentStartAtMs = base,
        currentEndAtMs = base + 90 * minute,
        nextSessionId = "a|2026-11-17|1000",
        nextStartAtMs = base,
        nextEndAtMs = base + 90 * minute,
        casting = true,
        now = base + 10 * minute,
      ),
    )
    // 课程变了、时间变了、已下课、没在投放：都要重建会话。
    assertFalse(
      LiveReminderSession.canReuseSession(
        "a", base, base + 90 * minute, "b", base, base + 90 * minute, true, base,
      ),
    )
    assertFalse(
      LiveReminderSession.canReuseSession(
        "a", base, base + 90 * minute, "a", base + minute, base + 90 * minute, true, base,
      ),
    )
    assertFalse(
      LiveReminderSession.canReuseSession(
        "a", base, base + 90 * minute, "a", base, base + 90 * minute, true, base + 200 * minute,
      ),
    )
    assertFalse(
      LiveReminderSession.canReuseSession(
        "a", base, base + 90 * minute, "a", base, base + 90 * minute, false, base,
      ),
    )
  }

  @Test
  fun colorOsVendorsNeedFirstPostReinforcement() {
    assertTrue(LiveReminderSession.needsFirstPostReinforcement("OPPO"))
    assertTrue(LiveReminderSession.needsFirstPostReinforcement("OnePlus"))
    assertTrue(LiveReminderSession.needsFirstPostReinforcement(" realme "))
    assertFalse(LiveReminderSession.needsFirstPostReinforcement("Xiaomi"))
    assertFalse(LiveReminderSession.needsFirstPostReinforcement(""))
  }
}
