package `in`.rinne.your_szu_life

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** 实时活动纯逻辑：课前窗口、进度、胶囊文本、重发节奏与心跳判断。 */
class LiveReminderSessionTest {
  private val minute = 60_000L
  private val base = 1_800_000_000_000L

  @Test
  fun progressMovesFromZeroToHundredAcrossTheLeadWindow() {
    assertEquals(0, LiveReminderSession.progressPercent(base, 20, base - 20 * minute))
    assertEquals(50, LiveReminderSession.progressPercent(base, 20, base - 10 * minute))
    assertEquals(100, LiveReminderSession.progressPercent(base, 20, base))
    // 窗口之外夹紧：起点前 0，上课后 100（上课后不再发布）。
    assertEquals(100, LiveReminderSession.progressPercent(base, 20, base + 30 * minute))
    assertEquals(0, LiveReminderSession.progressPercent(base, 20, base - 40 * minute))
  }

  @Test
  fun remainingMinutesRoundsUpSoTheLastMinuteStillShowsOne() {
    assertEquals(20, LiveReminderSession.remainingMinutes(base, base - 20 * minute))
    assertEquals(1, LiveReminderSession.remainingMinutes(base, base - minute))
    assertEquals(1, LiveReminderSession.remainingMinutes(base, base - 30_000L))
    assertEquals(0, LiveReminderSession.remainingMinutes(base, base))
  }

  @Test
  fun statusTextCountsDownAndHasAStartFallback() {
    assertEquals("还有 20 分钟", LiveReminderSession.statusText(base, base - 20 * minute))
    assertEquals("还有 1 分钟", LiveReminderSession.statusText(base, base - minute))
    // 胶囊与正文共用同一口径：最后一分钟不再提前说「马上开始」。
    assertEquals("还有 1 分钟", LiveReminderSession.statusText(base, base - 30_000L))
    // 上课时间到之后由清理流程接手，这里只保证函数是全的。
    assertEquals("即将开始", LiveReminderSession.statusText(base, base))
  }

  @Test
  fun notificationTextsPutTimeAndPlaceInTheTitleAndCountDownInTheBody() {
    val texts = LiveReminderSession.notificationTexts(
      timeLabel = "08:30–10:05",
      location = "致理楼 A101",
      courseName = "概率论",
      startAtMs = base,
      now = base - 14 * minute,
    )
    assertEquals("08:30–10:05 致理楼 A101", texts.title)
    assertEquals("概率论", texts.subText)
    assertEquals("14 分钟后上课", texts.text)
  }

  @Test
  fun notificationTextsDropThePlaceWhenThereIsNone() {
    val texts = LiveReminderSession.notificationTexts(
      timeLabel = "08:30–10:05",
      location = "  ",
      courseName = "概率论",
      startAtMs = base,
      now = base - minute,
    )
    assertEquals("08:30–10:05", texts.title)
  }

  @Test
  fun notificationCountdownRoundsUpSoItNeverSaysZeroMinutes() {
    // 最后一分钟仍显示「1 分钟后上课」，倒计时按分钟向上取整。
    assertEquals(
      "1 分钟后上课",
      LiveReminderSession.notificationTexts("08:30", "", "概率论", base, base - 30_000L).text,
    )
    assertEquals(
      "20 分钟后上课",
      LiveReminderSession.notificationTexts("08:30", "", "概率论", base, base - 20 * minute).text,
    )
  }

  @Test
  fun notificationTextsNeverShowAnInClassState() {
    val texts = LiveReminderSession.notificationTexts(
      timeLabel = "08:30–10:05",
      location = "致理楼 A101",
      courseName = "概率论",
      startAtMs = base,
      now = base + 5 * minute,
    )
    // 上课后不再发布通知；即便被误调用也不会出现「上课中」。
    assertEquals("即将开始", texts.text)
    assertEquals("概率论", texts.subText)
  }

  @Test
  fun capsuleTextPrefersCountdownThenFallsBackToTheCourseName() {
    assertEquals("14分", LiveReminderSession.capsuleText("概率论", "还有 14 分钟"))
    assertEquals("即将开始", LiveReminderSession.capsuleText("概率论", "即将开始"))
    // 非倒计时状态回退课程名，并截到 7 个字符（胶囊显示宽度限制）。
    assertEquals("概率论与数理统", LiveReminderSession.capsuleText("概率论与数理统计", "其它"))
    assertEquals("", LiveReminderSession.capsuleText("   ", "其它"))
  }

  @Test
  fun serviceTickNeverRunsPastTheStart() {
    assertEquals(
      LiveReminderSession.TICK_MS,
      LiveReminderSession.nextTickDelay(base, base + 10 * minute),
    )
    assertEquals(5_000L, LiveReminderSession.nextTickDelay(base, base + 5_000L))
    assertEquals(0L, LiveReminderSession.nextTickDelay(base, base))
  }

  @Test
  fun alarmTickIsCappedAtOneMinuteAndTheStart() {
    assertEquals(
      LiveReminderSession.ALARM_TICK_MS,
      LiveReminderSession.nextAlarmDelay(base, base + 10 * minute),
    )
    assertEquals(8_000L, LiveReminderSession.nextAlarmDelay(base, base + 8_000L))
    assertEquals(0L, LiveReminderSession.nextAlarmDelay(base, base))
  }

  @Test
  fun liveUpdateIsStaleWhenNotCastingOrTheHeartbeatExpired() {
    assertTrue(LiveReminderSession.liveUpdateStale(casting = false, lastPublishedAtMs = base, now = base))
    assertTrue(LiveReminderSession.liveUpdateStale(casting = true, lastPublishedAtMs = 0L, now = base))
    assertFalse(
      LiveReminderSession.liveUpdateStale(
        casting = true,
        lastPublishedAtMs = base,
        now = base + LiveReminderSession.STALE_MS - 1,
      ),
    )
    assertTrue(
      LiveReminderSession.liveUpdateStale(
        casting = true,
        lastPublishedAtMs = base,
        now = base + LiveReminderSession.STALE_MS,
      ),
    )
  }

  @Test
  fun sessionIsReusedOnlyForTheSameClassBeforeItStarts() {
    assertTrue(
      LiveReminderSession.canReuseSession(
        currentSessionId = "a|2026-11-17|1000",
        currentStartAtMs = base,
        nextSessionId = "a|2026-11-17|1000",
        nextStartAtMs = base,
        casting = true,
        now = base - 10 * minute,
      ),
    )
    // 课程变了、开始时间变了、已上课、没在投放：都要重建会话。
    assertFalse(
      LiveReminderSession.canReuseSession("a", base, "b", base, true, base - minute),
    )
    assertFalse(
      LiveReminderSession.canReuseSession("a", base, "a", base + minute, true, base - minute),
    )
    assertFalse(
      LiveReminderSession.canReuseSession("a", base, "a", base, true, base),
    )
    assertFalse(
      LiveReminderSession.canReuseSession("a", base, "a", base, false, base - minute),
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
