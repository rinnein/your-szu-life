package `in`.rinne.your_szu_life

import org.junit.Assert.*
import org.junit.Test

class BrowserChromePolicyTest {
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

  @Test fun theNativeBrowserBarOnlyServesExternalPages() {
    // 本地页面用 Web 顶栏；统一认证、自助服务、官方网费等外部页面才挂原生栏。
    assertFalse(shouldShowBrowserToolbar(true))
    assertTrue(shouldShowBrowserToolbar(false))
  }

  @Test fun chromeSurfaceAcceptsTheThemeHexColors() {
    assertEquals(0xFFFCF8FF.toInt(), parseCssColor("#fcf8ff"))
    assertEquals(0xFF151218.toInt(), parseCssColor(" #151218 "))
    assertEquals(0xFFFFFFFF.toInt(), parseCssColor("#FFF"))
    assertEquals(0xFF000000.toInt(), parseCssColor("#000000"))
  }

  @Test fun chromeSurfaceIgnoresAnythingButHex() {
    assertNull(parseCssColor(null))
    assertNull(parseCssColor(""))
    assertNull(parseCssColor("fcf8ff"))
    assertNull(parseCssColor("#fcf8f"))
    assertNull(parseCssColor("#fcf8ff00"))
    assertNull(parseCssColor("rgb(252,248,255)"))
    assertNull(parseCssColor("#gggggg"))
  }

  @Test fun statusBarTakesTheTopBarColourWhileTheGestureBarStaysFixed() {
    val toolbarColor = 0xFF151218.toInt()
    val scrolledTopBar = 0xFF131318.toInt()
    val themeSurface = 0xFFFCF8FF.toInt()
    val themeContainer = 0xFFF6F2FA.toInt()
    // 本地页面：顶部条是 Web 顶栏，颜色由 Web 端随滚动推送（surface → surface-container）。
    assertEquals(scrolledTopBar, statusBarColorFor(false, toolbarColor, scrolledTopBar, themeSurface))
    // 外部页面：顶部条是原生浏览器栏，取它自己的颜色，忽略 Web 端推来的值。
    assertEquals(toolbarColor, statusBarColorFor(true, toolbarColor, scrolledTopBar, themeSurface))
    // 还没收到推送时退回主题色。
    assertEquals(themeSurface, statusBarColorFor(false, toolbarColor, null, themeSurface))
    // 小白条固定取底部导航栏的颜色，与顶栏、与滚动都无关。
    assertEquals(themeContainer, navigationBarColorFor(null, themeContainer))
    assertEquals(0xFFE8DEF8.toInt(), navigationBarColorFor(0xFFE8DEF8.toInt(), themeContainer))
  }

  @Test fun lightSurfacesTakeDarkSystemIcons() {
    assertTrue(isLightSurface(0xFFFFFFFF.toInt()))
    assertTrue(isLightSurface(0xFFFCF8FF.toInt()))
    assertFalse(isLightSurface(0xFF000000.toInt()))
    assertFalse(isLightSurface(0xFF151218.toInt()))
    // 相对亮度按 sRGB 线性化：中灰 0x808080 只有约 0.22，仍取浅色图标。
    assertFalse(isLightSurface(0xFF808080.toInt()))
  }
}
