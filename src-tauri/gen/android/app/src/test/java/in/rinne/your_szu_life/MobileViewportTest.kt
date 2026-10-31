package `in`.rinne.your_szu_life

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pure-policy tests; 写入 `WebSettings` 那半边要真 WebView，仍留在手工验收里。
 * 去重判定是纯函数，用任意对象代表「WebView 实例」就能测。
 */
class MobileViewportTest {
  /** 只要能区分「同一个实例 / 不同实例」即可，判定只做 `===`。 */
  private fun webView(): Any = Any()

  @Test
  fun internalPagesLockEveryZoomSwitch() {
    val zoom = zoomSettings(localDocument = true)
    assertFalse(zoom.supportZoom)
    assertFalse(zoom.builtInZoomControls)
    assertFalse(zoom.displayZoomControls)
    assertFalse(zoom.useWideViewPort)
    assertFalse(zoom.loadWithOverviewMode)
    // 内部页面是 flex 布局，文本缩放同样会顶坏排版。
    assertEquals(100, zoom.textZoom)
  }

  @Test
  fun externalPagesKeepPinchZoom() {
    val zoom = zoomSettings(localDocument = false)
    assertTrue(zoom.supportZoom)
    assertTrue(zoom.builtInZoomControls)
    assertTrue(zoom.useWideViewPort)
    assertTrue(zoom.loadWithOverviewMode)
    // 缩放按钮不显示，手势就够了。
    assertFalse(zoom.displayZoomControls)
  }

  @Test
  fun externalPagesLeaveTheSystemTextScaleAlone() {
    // null 表示不写 textZoom：官方门户的小字跟随用户系统字号。
    assertNull(zoomSettings(localDocument = false).textZoom)
  }

  @Test
  fun theTwoPoliciesAreCompleteSnapshotsNotPatches() {
    val locked = zoomSettings(localDocument = true)
    val open = zoomSettings(localDocument = false)
    // 两个分支都必须对每一项给出明确取值，否则换文档时会残留上一个页面的设置。
    assertNotEquals(locked, open)
    assertTrue(open.supportZoom != locked.supportZoom)
    assertTrue(open.useWideViewPort != locked.useWideViewPort)
    assertTrue(open.builtInZoomControls != locked.builtInZoomControls)
    assertTrue(open.loadWithOverviewMode != locked.loadWithOverviewMode)
    assertTrue(open.textZoom != locked.textZoom)
  }

  @Test
  fun theTwoPoliciesAreStableAcrossCalls() {
    // 每次导航都会重放进外部页/回本地页，策略必须是同一个常量，不能每次新建。
    assertTrue(zoomSettings(true) === zoomSettings(true))
    assertTrue(zoomSettings(false) === zoomSettings(false))
  }

  @Test
  fun aFreshWebViewAlwaysGetsItsFirstWrite() {
    // 首次挂载：无论类别如何都必须写，否则默认的 supportZoom = true 会漏过去。
    assertTrue(shouldWrite(null, null, webView(), localDocument = true))
    assertTrue(shouldWrite(null, null, webView(), localDocument = false))
  }

  @Test
  fun repeatedNoticesOnTheSameDocumentAreDeduped() {
    // 同一个 WebView、同一类别：不重写，保住用户自己捏合出来的缩放。
    val view = webView()
    assertFalse(shouldWrite(view, true, view, localDocument = true))
    assertFalse(shouldWrite(view, false, view, localDocument = false))
  }

  @Test
  fun flippingTheDocumentCategoryRewrites() {
    val view = webView()
    assertTrue(shouldWrite(view, true, view, localDocument = false))
    assertTrue(shouldWrite(view, false, view, localDocument = true))
  }

  @Test
  fun aRebuiltWebViewIsNeverTreatedAsAlreadyApplied() {
    // 门户窗口每次都是新 WebView：上一个实例记为「内部页已锁」时，
    // 新实例的首帧锁死仍必须发生。
    val previous = webView()
    assertTrue(shouldWrite(previous, true, webView(), localDocument = true))
    // 反过来也一样：上一个实例停在外部页，新实例要重新判定。
    assertTrue(shouldWrite(previous, false, webView(), localDocument = false))
  }
}
