package `in`.rinne.your_szu_life

import android.os.Handler
import android.os.Looper
import android.webkit.CookieManager
import androidx.webkit.CookieManagerCompat
import androidx.webkit.WebViewFeature
import org.json.JSONArray
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** Native-only cookie transport. Cookie values never enter JavaScript or logs. */
object AuthCookieBridge {
  @JvmStatic
  fun supportsCookieInfo(): Boolean =
    WebViewFeature.isFeatureSupported(WebViewFeature.GET_COOKIE_INFO)

  @JvmStatic
  fun readCookies(urlsJson: String): String {
    check(supportsCookieInfo()) { "cookie-info-unavailable" }
    val urls = JSONArray(urlsJson)
    val result = JSONArray()
    val manager = CookieManager.getInstance()
    for (index in 0 until urls.length()) {
      for (cookie in CookieManagerCompat.getCookieInfo(manager, urls.getString(index))) {
        result.put(cookie)
      }
    }
    return result.toString()
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
