package `in`.rinne.your_szu_life

import android.os.Bundle
import android.content.res.Configuration
import android.view.View
import android.view.WindowManager
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge

/** Shared by every app-owned Tauri Activity, including child windows. */
abstract class ApplicationBackActivity : TauriActivity(), app.tauri.AppBackNavigationOwner {
  final override val handleBackNavigation: Boolean = false

  private var applicationBack: ApplicationBackNavigation? = null
  private var chrome: NativeWebViewChrome? = null
  private var resumed = false

  internal val nativeToolbarReady: Boolean get() = chrome != null

  internal fun updateToolbarBack(hasLogicalParent: Boolean) {
    chrome?.showBack(this is FeatureActivity || hasLogicalParent)
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Required for IME insets on older Android; never pan the toolbar under
    // the status bar when focusing a field near the bottom of a portal.
    window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    applicationBack?.dispose()
    applicationBack = ApplicationBackNavigation(this, webView)
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
    resumed = true
    chrome?.setResumed(true)
  }

  override fun onPause() {
    resumed = false
    chrome?.setResumed(false)
    super.onPause()
  }

  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    enableEdgeToEdge()
    chrome?.refreshTheme()
  }

  override fun onDestroy() {
    chrome?.dispose()
    chrome = null
    applicationBack?.dispose()
    applicationBack = null
    super.onDestroy()
  }
}
