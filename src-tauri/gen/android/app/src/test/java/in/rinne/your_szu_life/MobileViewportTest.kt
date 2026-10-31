package `in`.rinne.your_szu_life

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Pure-policy tests; writing `WebSettings` needs a real WebView, so that half stays out. */
class MobileViewportTest {
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
}
