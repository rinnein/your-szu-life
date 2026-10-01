package `in`.rinne.your_szu_life

import android.os.Bundle
import android.content.res.Configuration
import android.view.View
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

  internal val nativeToolbarReady: Boolean get() = chrome != null

  internal fun updateToolbarBack(hasLogicalParent: Boolean) {
    chrome?.showBack(authReturnUrl != null || this is FeatureActivity || hasLogicalParent)
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    AuthWindowBridge.created(this)
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
    next.showBack(this is FeatureActivity)
    next.setResumed(resumed)
    applicationBack?.toolbarMounted()
  }

  override fun onResume() {
    super.onResume()
    UoocTaskBridge.resumed()
    resumed = true
    AuthWindowBridge.resumed(this)
    chrome?.setResumed(true)
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
  }

  override fun onDestroy() {
    AuthWindowBridge.destroyed(this)
    authWebView = null
    authBack.remove()
    chrome?.dispose()
    chrome = null
    applicationBack?.dispose()
    applicationBack = null
    super.onDestroy()
  }
}
