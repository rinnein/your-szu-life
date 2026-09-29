package `in`.rinne.your_szu_life

import org.junit.Assert.*
import org.junit.Test

class BrowserChromePolicyTest {
  @Test fun safeAreaDoesNotAddKeyboardAndNavigationBarTwice() {
    assertEquals(300, browserBottomInset(24, 0, 300))
    assertEquals(48, browserBottomInset(24, 48, 0))
    assertEquals(24, browserBottomInset(24, 0, 0))
  }

  @Test fun titlesRemainPlainTextWithAnEmptyPageFallback() {
    assertEquals("泥深", browserTitle(null))
    assertEquals("泥深", browserTitle("  "))
    assertEquals("网费", browserTitle(" 网费 "))
    assertEquals("<img src=x>", browserTitle("<img src=x>"))
  }

  @Test fun rootOverlayDoesNotExposeAnExitArrowAndRemotePagesCannotClaimAParent() {
    val origin = "http://tauri.localhost"
    val policy = ApplicationBackPolicy(setOf(origin))
    policy.document(origin)
    policy.logicalParent(false, origin, origin, true)
    policy.localState("overlay", origin, origin, true)
    assertTrue(policy.enabled)
    assertFalse(policy.hasLogicalParent)
    policy.logicalParent(true, origin, origin, true)
    policy.localState("overlay", origin, origin, true)
    assertTrue(policy.hasLogicalParent)
    policy.document("https://authserver.szu.edu.cn/authserver/login")
    assertFalse(policy.hasLogicalParent)
    assertFalse(policy.logicalParent(true, "https://authserver.szu.edu.cn", origin, true))
    assertFalse(policy.logicalParent(true, origin, origin, false))
    assertFalse(policy.hasLogicalParent)
  }
}
