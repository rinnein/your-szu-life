package `in`.rinne.your_szu_life

import android.os.Bundle
import android.content.res.Configuration
import android.view.View
import android.view.Window
import android.view.WindowManager
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.activity.OnBackPressedCallback

/** Shared by every app-owned Tauri Activity, including child windows. */
abstract class ApplicationBackActivity : TauriActivity(), app.tauri.AppBackNavigationOwner {
  final override val handleBackNavigation: Boolean = false

  private var applicationBack: ApplicationBackNavigation? = null
  private var chrome: NativeWebViewChrome? = null
  private var resumed = false
  private var authReturnUrl: String? = null
  private var authWebView: WebView? = null
  private var authenticationPresented = false
  /** Web 端最近推来的系统栏颜色，跨 WebView 重建保留。 */
  private var pushedStatusBarColor: Int? = null
  private var pushedNavigationBarColor: Int? = null
  internal val authenticationContent: View? get() = chrome?.root

  internal fun setAuthenticationPresented(value: Boolean) {
    authenticationPresented = value
    chrome?.setResumed(resumed || value)
    if (value || resumed) authWebView?.onResume() else authWebView?.onPause()
  }

  internal fun returnFromAuthentication() {
    val destination = authReturnUrl ?: return
    authWebView?.loadUrl(destination)
    setAuthReturnUrl(null)
  }

  internal fun requestPageBack() {
    if (authReturnUrl != null) returnFromAuthentication()
    else onBackPressedDispatcher.onBackPressed()
  }

  private val authBack = object : OnBackPressedCallback(false) {
    override fun handleOnBackPressed() {
      // Leave the official document; Rust cancels this recovery generation.
      returnFromAuthentication()
    }
  }

  internal fun setAuthReturnUrl(url: String?) {
    authReturnUrl = url
    authBack.isEnabled = url != null
    if (url != null) chrome?.showBack(true)
  }

  internal val nativeToolbarReady: Boolean get() = chrome?.browserToolbarVisible == true

  /**
   * 只有外部（非 Tauri 本地）页面才显示原生浏览器栏。
   *
   * 本地页面用 Web 组件提供的顶栏：它随主题、导航层级一起变化，且是页面的一部分；
   * 原生栏留给受控打开的统一认证、自助服务、官方网费等外部页面，它们需要自己的
   * 标题与返回/刷新入口，也不受应用路由控制。
   */
  internal fun updateBrowserToolbar(localDocument: Boolean) {
    val chrome = chrome ?: return
    chrome.setBrowserToolbarVisible(shouldShowBrowserToolbar(localDocument))
  }

  /**
   * Web 端推来的状态栏颜色（上方顶栏的当前颜色，滚动时会变）。
   * 传 null 表示退回主题色。
   */
  internal fun setChromeStatusBarColor(color: Int?) {
    pushedStatusBarColor = color
    applySystemBars()
  }

  /** Web 端推来的小白条颜色（固定取底部导航栏的颜色）。传 null 表示退回主题色。 */
  internal fun setChromeNavigationBarColor(color: Int?) {
    pushedNavigationBarColor = color
    applySystemBars()
  }

  private fun applySystemBars() {
    // 内容区自己画上下两条系统栏区域（Android 15 起系统栏不再上色），
    // 窗口属性只是旧版本系统上的等价做法，图标明暗由 SystemBars 一起决定。
    val (statusBar, navigationBar) = resolveSystemBarColors(window)
    chrome?.setSystemBarColors(statusBar, navigationBar)
    SystemBars.apply(window, statusBar, navigationBar)
    if (authenticationPresented) {
      SystemBars.apply(AuthWindowBridge.presentationWindow, statusBar, navigationBar)
    }
  }

  /** 沉浸式取色的唯一入口：状态栏取顶部条颜色，小白条取底部导航栏颜色。 */
  private fun resolveSystemBarColors(target: Window): Pair<Int, Int> {
    val statusBar = statusBarColorFor(
      toolbarVisible = chrome?.browserToolbarVisible == true,
      toolbarColor = chrome?.surfaceColor ?: SystemBars.themeSurface(target),
      pushedColor = pushedStatusBarColor,
      themeFallback = SystemBars.themeSurface(target),
    )
    val navigationBar =
      navigationBarColorFor(pushedNavigationBarColor, SystemBars.themeSurfaceContainer(target))
    return statusBar to navigationBar
  }

  /** 认证浮层换成另一个窗口后，用它自己的主题色重新解析一次。 */
  internal fun applySystemBarsTo(target: Window?) {
    if (target == null) return
    val (statusBar, navigationBar) = resolveSystemBarColors(target)
    SystemBars.apply(target, statusBar, navigationBar)
  }

  internal fun updateToolbarBack(hasLogicalParent: Boolean) {
    chrome?.showBack(authReturnUrl != null || this is FeatureActivity || hasLogicalParent)
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    // Must run before super.onCreate so the activity inflates with the
    // wallpaper-derived colours; NativeWebViewChrome reads them from the theme.
    com.google.android.material.color.DynamicColors.applyToActivityIfAvailable(this)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    AuthWindowBridge.created(this)
    WallpaperColorBridge.bindActivity(this)
    // 边到边绘制是 Android 15 的强制行为：系统栏不会自己上色，由应用把这条带状区域
    // 画成与 WebView 相同的底色（Web 端推送 chrome-surface 后即换成页面真实底色）。
    applySystemBars()
    // Required for IME insets on older Android; never pan the toolbar under
    // the status bar when focusing a field near the bottom of a portal.
    @Suppress("DEPRECATION") // Retained for Android 7-10; modern layout uses WindowInsetsCompat.
    window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    applicationBack?.dispose()
    applicationBack = ApplicationBackNavigation(this, webView)
    authWebView = webView
    authBack.remove()
    onBackPressedDispatcher.addCallback(this, authBack)
  }

  // Wry calls this after onWebViewCreate and after installing its clients.
  override fun setContentView(view: View?) {
    if (view !is WebView) {
      super.setContentView(view)
      return
    }
    chrome?.dispose()
    val next = NativeWebViewChrome(this, view)
    chrome = next
    super.setContentView(next.root)
    next.setResumed(resumed)
    // 原生栏初始隐藏（默认本地页面）：等首个 document 通知判定本地/外部页面。
    applySystemBars()
    applicationBack?.toolbarMounted()
  }

  override fun onResume() {
    super.onResume()
    UoocTaskBridge.resumed()
    resumed = true
    AuthWindowBridge.resumed(this)
    chrome?.setResumed(true)
    // 主题可能在后台期间被改动过（壁纸换色没有广播，只能在这里补查）。
    applySystemBars()
    // The user may have changed the wallpaper while we were backgrounded.
    // There is no broadcast for it, so poll here and tell the WebView to
    // regenerate its palette.
    if (WallpaperColorBridge.hasSeedChanged()) {
      applicationBack?.notifyWallpaperChanged()
    }
  }

  override fun onPause() {
    resumed = false
    AuthWindowBridge.paused(this)
    chrome?.setResumed(authenticationPresented)
    super.onPause()
  }

  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    enableEdgeToEdge()
    chrome?.refreshTheme()
    applySystemBars()
  }

  override fun onDestroy() {
    AuthWindowBridge.destroyed(this)
    WallpaperColorBridge.unbindActivity()
    authWebView = null
    authBack.remove()
    chrome?.dispose()
    chrome = null
    applicationBack?.dispose()
    applicationBack = null
    super.onDestroy()
  }
}
