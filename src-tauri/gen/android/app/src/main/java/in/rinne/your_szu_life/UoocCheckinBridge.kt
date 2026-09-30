package `in`.rinne.your_szu_life

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Bitmap
import androidx.core.net.toUri
import android.net.http.SslError
import android.os.Handler
import android.os.Looper
import android.webkit.SslErrorHandler
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import java.util.concurrent.ConcurrentHashMap

internal fun isUoocCheckinCourseUrl(value: String): Boolean = runCatching {
  val uri = java.net.URI(value)
  uri.scheme == "https" && uri.host == "www.uooc.net.cn" && uri.userInfo == null &&
    (uri.port == -1 || uri.port == 443) && Regex("/home/course/[1-9][0-9]*").matches(uri.path)
}.getOrDefault(false)

/** Hidden page loading only. No Activity, JS interface, or cookie values cross this bridge. */
object UoocCheckinBridge {
  private class Page(val url: String) {
    @Volatile var state = "loading"
    @Volatile var view: WebView? = null
  }
  private val pages = ConcurrentHashMap<String, Page>()
  private val main = Handler(Looper.getMainLooper())

  @JvmStatic
  @SuppressLint("SetJavaScriptEnabled") // The official site performs its own check-in after JS loads.
  fun open(context: Context, id: String, url: String): Boolean {
    if (!isUoocCheckinCourseUrl(url) || !id.startsWith("uooc-checkin-")) return false
    val page = Page(url)
    if (pages.putIfAbsent(id, page) != null) return false
    main.post {
      if (pages[id] !== page) return@post
      try {
        val view = WebView(context.applicationContext)
        page.view = view
        if (pages[id] !== page) { view.destroy(); return@post }
        view.settings.apply {
          javaScriptEnabled = true
          domStorageEnabled = true
          allowFileAccess = false
          allowContentAccess = false
          mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
          mediaPlaybackRequiresUserGesture = true
        }
        view.webViewClient = object : WebViewClient() {
          private fun expected(value: String): Boolean {
            val uri = value.toUri()
            return isUoocCheckinCourseUrl(value) && uri.path == page.url.toUri().path
          }
          override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            if (request.isForMainFrame && !expected(request.url.toString())) {
              page.state = "login"
              return true
            }
            return false
          }
          override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
            if (pages[id] === page) page.state = if (expected(url)) "loading" else "login"
          }
          override fun onPageFinished(view: WebView, url: String) {
            if (pages[id] === page && page.state != "failed") page.state = if (expected(url)) "ready" else "login"
          }
          override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
            if (request.isForMainFrame) page.state = "failed"
          }
          override fun onReceivedHttpError(view: WebView, request: WebResourceRequest, response: WebResourceResponse) {
            if (request.isForMainFrame) page.state = "failed"
          }
          override fun onReceivedSslError(view: WebView, handler: SslErrorHandler, error: SslError) {
            page.state = "failed"
            handler.cancel()
          }
          override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
            page.state = "failed"
            page.view = null
            runCatching { view.destroy() }
            return true
          }
        }
        view.onResume()
        view.loadUrl(url)
      } catch (_: Exception) { page.state = "failed" }
    }
    return true
  }

  @JvmStatic fun state(id: String): String = pages[id]?.state ?: "closed"

  @JvmStatic fun close(id: String) {
    val page = pages.remove(id) ?: return
    main.post {
      runCatching { page.view?.apply { stopLoading(); onPause(); destroy() } }
      page.view = null
    }
  }
}
