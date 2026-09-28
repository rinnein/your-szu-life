package `in`.rinne.your_szu_life

import org.junit.Assert.*
import org.junit.Test

class ApplicationBackPolicyTest {
  private val origin = "http://tauri.localhost"
  private fun policy() = ApplicationBackPolicy(setOf(origin))

  @Test fun entryOfEveryActivityYieldsRegardlessOfItsUrl() {
    for (path in listOf("/", "/settings", "/campus-payments/network")) {
      val policy = policy()
      policy.document("$origin$path")
      assertFalse(policy.enabled)
      assertTrue(policy.localState("system", origin, "$origin$path", true))
      assertFalse(policy.enabled)
      assertNull(policy.action)
    }
  }

  @Test fun onlyExplicitLogicalParentEnablesPagePop() {
    val policy = policy()
    assertTrue(policy.localState("route", origin, "$origin/settings", true))
    assertTrue(policy.enabled)
    assertEquals("navigate-back", policy.action)
    policy.localState("system", origin, "$origin/settings", true)
    assertFalse(policy.enabled)
  }

  @Test fun overlayReleasesNativeParentWhenClosed() {
    val policy = policy()
    policy.localState("overlay", origin, "$origin/settings", true)
    assertEquals("dismiss-overlay", policy.action)
    assertTrue(policy.enabled)
    policy.localState("system", origin, "$origin/settings", true)
    assertFalse(policy.enabled)
    assertNull(policy.action)
  }

  @Test fun remoteAndLoadingNeverFallBackToWebViewHistory() {
    val policy = policy()
    policy.localState("overlay", origin, origin, true)
    policy.localState("loading", origin, origin, true)
    assertFalse(policy.enabled)
    policy.document("https://self.szu.edu.cn/self/nav_login")
    assertFalse(policy.enabled)
    assertFalse(policy.localDocument)
    assertNull(policy.action)
  }

  @Test fun subframesRemoteAndOldSourceCannotControlOrRequestBack() {
    val policy = policy()
    assertFalse(policy.acceptsSource(origin, origin, false))
    assertFalse(policy.acceptsSource("https://evil.example", "https://evil.example", true))
    assertFalse(policy.acceptsSource(origin, "https://authserver.szu.edu.cn/", true))
    assertFalse(policy.localState("overlay", "$origin.evil.example", origin, true))
    assertFalse(policy.localState("unknown", origin, origin, true))
    assertTrue(policy.acceptsSource(origin, "$origin/settings", true))
    assertFalse(policy.enabled)
  }

  @Test fun exactOriginsOnlyWithDefaultPortNormalization() {
    assertEquals(origin, applicationOrigin("HTTP://TAURI.LOCALHOST:80/settings?x=1#test"))
    assertEquals("https://tauri.localhost", applicationOrigin("https://tauri.localhost:443/"))
    assertNull(applicationOrigin("http://user@tauri.localhost/"))
    assertNull(applicationOrigin("javascript:alert(1)"))
    assertNull(applicationOrigin("not a URL"))
    assertNull(applicationOrigin(null))
    assertFalse(policy().localState("route", "$origin:1420", "$origin:1420", true))
  }

  @Test fun activityStatesAreIndependent() {
    val root = policy()
    val child = policy()
    child.localState("overlay", origin, origin, true)
    assertFalse(root.enabled)
    assertTrue(child.enabled)
  }
}
