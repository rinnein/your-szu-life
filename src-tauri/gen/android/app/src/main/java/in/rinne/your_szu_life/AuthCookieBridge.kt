package `in`.rinne.your_szu_life

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.webkit.CookieManager
import androidx.webkit.CookieManagerCompat
import androidx.webkit.WebViewFeature
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.net.URI

/** Native-only cookie transport. Cookie values never enter JavaScript or logs. */
object AuthCookieBridge {
  @Volatile private var applicationContext: Context? = null

  fun bindContext(context: Context) { applicationContext = context.applicationContext }

  @JvmStatic
  fun showAuthentication(returnUrl: String): Boolean = AuthWindowBridge.showAuthentication(returnUrl)

  @JvmStatic
  fun finishAuthentication(restore: Boolean): Boolean = AuthWindowBridge.finishAuthentication(restore)

  @JvmStatic
  fun supportsCookieCapture(): Boolean =
    WebViewFeature.isFeatureSupported(WebViewFeature.GET_COOKIE_INFO) ||
      applicationContext != null

  @JvmStatic
  fun usesLegacyCookieCapture(): Boolean =
    !WebViewFeature.isFeatureSupported(WebViewFeature.GET_COOKIE_INFO)

  @JvmStatic
  @Synchronized
  fun readCookies(urlsJson: String): String {
    var backend = "legacy"
    return try {
      if (Looper.myLooper() == Looper.getMainLooper()) throw CookieReadException(CookieReadError.NATIVE_FAILURE)
      val legacy = usesLegacyCookieCapture()
      backend = if (legacy) "legacy" else "native"
      val urls = JSONArray(urlsJson)
      val requested = (0 until urls.length()).map { URI(urls.getString(it)) }
      val cookies = if (legacy) {
        val context = applicationContext ?: throw CookieReadException(CookieReadError.NATIVE_FAILURE)
        LegacyAuthCookieStore(context).read(requested)
      } else {
        val manager = CookieManager.getInstance()
        requested.flatMap { CookieManagerCompat.getCookieInfo(manager, it.toString()) }
      }
      JSONObject().put("status", "ok").put("cookies", JSONArray(cookies)).toString()
    } catch (failure: Exception) {
      val code = (failure as? CookieReadException)?.code ?: CookieReadError.NATIVE_FAILURE
      // Never serialize an Android exception or its message.
      JSONObject().put("status", "error").put("backend", backend).put("code", code.wireName).toString()
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
