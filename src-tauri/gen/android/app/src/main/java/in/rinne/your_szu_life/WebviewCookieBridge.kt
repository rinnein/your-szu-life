package `in`.rinne.your_szu_life

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.Process
import android.webkit.CookieManager
import android.webkit.WebView
import androidx.webkit.WebViewFeature
import androidx.webkit.CookieManagerCompat
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** Native-only transport; never exposed as a JavaScript interface. */
object WebviewCookieBridge {
  @Volatile private var applicationContext: Context? = null
  fun bindContext(context: Context) { applicationContext = context.applicationContext }

  @JvmStatic fun supportsCookieInfo(): Boolean =
    WebViewFeature.isFeatureSupported(WebViewFeature.GET_COOKIE_INFO)

  @JvmStatic fun prepareDevTools(): Boolean {
    if (applicationContext == null) return false
    if (Looper.myLooper() == Looper.getMainLooper()) {
      WebView.setWebContentsDebuggingEnabled(true)
      return true
    }
    val finished = CountDownLatch(1)
    val succeeded = AtomicBoolean(false)
    Handler(Looper.getMainLooper()).post {
      try { WebView.setWebContentsDebuggingEnabled(true); succeeded.set(true) }
      catch (_: Exception) { succeeded.set(false) }
      finally { finished.countDown() }
    }
    return finished.await(2, TimeUnit.SECONDS) && succeeded.get()
  }

  @JvmStatic fun devToolsSocketName(): String = "webview_devtools_remote_${Process.myPid()}"

  @JvmStatic
  @Synchronized
  fun readHeaders(urlsJson: String): String {
    check(Looper.myLooper() != Looper.getMainLooper()) { "cookie-read-on-main-thread" }
    val urls = JSONArray(urlsJson)
    val manager = CookieManager.getInstance()
    val result = JSONArray()
    for (index in 0 until urls.length()) result.put(manager.getCookie(urls.getString(index)).orEmpty())
    return result.toString()
  }

  /** Preserve the working AndroidX read API; never require DevTools here. */
  @JvmStatic
  @Synchronized
  fun readCookieInfo(urlsJson: String): String {
    fun failure(code: String): String = JSONObject().put("status", "error").put("code", code).toString()
    return try {
      if (Looper.myLooper() == Looper.getMainLooper()) return failure("Unavailable")
      if (!supportsCookieInfo()) return failure("UnsupportedMethod")
      val urls = JSONArray(urlsJson)
      val manager = CookieManager.getInstance()
      val cookies = JSONArray()
      for (index in 0 until urls.length()) {
        cookies.put(JSONArray(CookieManagerCompat.getCookieInfo(manager, urls.getString(index))))
      }
      JSONObject().put("status", "ok").put("cookies", cookies).toString()
    } catch (_: UnsupportedOperationException) {
      failure("UnsupportedMethod")
    } catch (_: Exception) {
      failure("Unavailable")
    }
  }

  /** Called on a Rust worker; callbacks run on Android's main looper. */
  @JvmStatic
  @Synchronized
  fun writeCookies(cookiesJson: String): Boolean {
    check(Looper.myLooper() != Looper.getMainLooper()) { "cookie-write-on-main-thread" }
    val cookies = JSONArray(cookiesJson)
    if (cookies.length() == 0) return true
    val finished = CountDownLatch(1)
    val cancelled = AtomicBoolean(false)
    val succeeded = AtomicBoolean(true)
    val manager = CookieManager.getInstance()
    fun writeNext(index: Int) {
      if (cancelled.get()) return
      if (index == cookies.length()) {
        finished.countDown()
        return
      }
      try {
        val cookie = cookies.getJSONObject(index)
        manager.setCookie(cookie.getString("url"), cookie.getString("header")) { accepted ->
          if (!accepted) succeeded.set(false)
          writeNext(index + 1)
        }
      } catch (_: Exception) {
        succeeded.set(false)
        finished.countDown()
      }
    }
    Handler(Looper.getMainLooper()).post { writeNext(0) }
    if (!finished.await(5, TimeUnit.SECONDS)) {
      cancelled.set(true)
      return false
    }
    manager.flush()
    return succeeded.get()
  }
}
