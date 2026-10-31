package `in`.rinne.your_szu_life

import android.webkit.WebSettings
import android.webkit.WebView

/**
 * 一次缩放设置的完整取值。
 *
 * 内部页面是应用自己的 flex 布局：宽度自适应视口，用户双指放大只会把排版顶出屏幕，
 * 所以缩放全线锁死。外部页面（统一认证、自助服务、官方网费等）是别人写的固定版心
 * 网页，小字多，必须留给用户放大，因此缩放全开、文本缩放跟随系统字号。
 */
internal data class ZoomSettings(
  val supportZoom: Boolean,
  val builtInZoomControls: Boolean,
  val displayZoomControls: Boolean,
  val useWideViewPort: Boolean,
  val loadWithOverviewMode: Boolean,
  /** null 表示不写回，保持系统字号缩放。 */
  val textZoom: Int?,
)

/** 内部页面：`setSupportZoom(false)` 必须配 `useWideViewPort = false` 才真正关掉手势缩放。 */
private val lockedZoom = ZoomSettings(
  supportZoom = false,
  builtInZoomControls = false,
  displayZoomControls = false,
  useWideViewPort = false,
  loadWithOverviewMode = false,
  textZoom = 100,
)

/** 外部页面：不显示缩放按钮（手势就够了），其余保持 WebView 默认的放开状态。 */
private val openZoom = ZoomSettings(
  supportZoom = true,
  builtInZoomControls = true,
  displayZoomControls = false,
  useWideViewPort = true,
  loadWithOverviewMode = true,
  textZoom = null,
)

/** 纯策略：按当前文档是否属于应用自身 origin 决定缩放取值。 */
internal fun zoomSettings(localDocument: Boolean): ZoomSettings =
  if (localDocument) lockedZoom else openZoom

/** 全应用唯一的缩放写入点。 */
internal fun applyZoomSettings(settings: WebSettings, zoom: ZoomSettings) {
  settings.setSupportZoom(zoom.supportZoom)
  settings.setBuiltInZoomControls(zoom.builtInZoomControls)
  settings.setDisplayZoomControls(zoom.displayZoomControls)
  settings.setUseWideViewPort(zoom.useWideViewPort)
  settings.setLoadWithOverviewMode(zoom.loadWithOverviewMode)
  zoom.textZoom?.let { settings.setTextZoom(it) }
}

/**
 * 按文档归属重放一次缩放策略。
 *
 * 只在文档真的换了类别时才写入：外部页面的文本缩放属于用户系统字号，
 * 每次 document 通知都重写会把用户自己捏合出来的缩放一并抹掉。
 */
internal class MobileViewport {
  private var applied: Boolean? = null

  fun apply(webView: WebView, localDocument: Boolean) {
    if (applied == localDocument) return
    applied = localDocument
    applyZoomSettings(webView.settings, zoomSettings(localDocument))
  }
}
