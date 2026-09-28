package `in`.rinne.your_szu_life

import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge

/** Shared by every app-owned Tauri Activity, including child windows. */
abstract class ApplicationBackActivity : TauriActivity(), app.tauri.AppBackNavigationOwner {
  final override val handleBackNavigation: Boolean = false

  private var applicationBack: ApplicationBackNavigation? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    applicationBack?.dispose()
    applicationBack = ApplicationBackNavigation(this, webView)
  }

  override fun onDestroy() {
    applicationBack?.dispose()
    applicationBack = null
    super.onDestroy()
  }
}
