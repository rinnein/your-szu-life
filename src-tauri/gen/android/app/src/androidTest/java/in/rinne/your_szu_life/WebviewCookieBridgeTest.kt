package `in`.rinne.your_szu_life

import android.os.Process
import android.webkit.CookieManager
import androidx.test.platform.app.InstrumentationRegistry
import androidx.webkit.WebViewFeature
import androidx.webkit.CookieManagerCompat
import org.junit.Assume.assumeTrue
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class WebviewCookieBridgeTest {
  private val context = InstrumentationRegistry.getInstrumentation().targetContext

  @Test fun exposesOnlyTheCurrentApplicationsDevToolsSocket() {
    WebviewCookieBridge.bindContext(context)
    assertTrue(WebviewCookieBridge.prepareDevTools())
    assertEquals("webview_devtools_remote_${Process.myPid()}", WebviewCookieBridge.devToolsSocketName())
    assertEquals(WebViewFeature.isFeatureSupported(WebViewFeature.GET_COOKIE_INFO), WebviewCookieBridge.supportsCookieInfo())
  }

  @Test fun nativeCookieWritesRetainHostDomainAndPathVariants() {
    val host = "webview-cookie-test.invalid"
    val origin = "https://$host"
    val name = "szu_cookie_fixture"
    val definitions = listOf(
      "$name=host; Path=/; Secure; HttpOnly",
      "$name=domain; Domain=$host; Path=/; Secure; HttpOnly",
      "$name=path; Path=/login; Secure; HttpOnly",
    )
    fun write(headers: List<String>): Boolean {
      val records = JSONArray()
      headers.forEach { records.put(JSONObject().put("url", "$origin/").put("header", it)) }
      return WebviewCookieBridge.writeCookies(records.toString())
    }
    try {
      assertTrue(write(definitions))
      val pairs = CookieManager.getInstance().getCookie("$origin/login").orEmpty().split(';').map { it.trim() }.filter { it.startsWith("$name=") }
      assertEquals(setOf("$name=host", "$name=domain", "$name=path"), pairs.toSet())
    } finally {
      assertTrue(write(definitions.map { "$it; Max-Age=0" }))
    }
  }

  @Test fun modernCookieInfoApiRetainsVirtualCookieAttributes() {
    assumeTrue("GET_COOKIE_INFO is unavailable on this WebView", WebviewCookieBridge.supportsCookieInfo())
    val origin = "https://webview-cookie-info-test.invalid/"
    val header = "szu_info_fixture=virtual; Path=/; Secure; HttpOnly; SameSite=Lax"
    fun write(value: String): Boolean = WebviewCookieBridge.writeCookies(
      JSONArray().put(JSONObject().put("url", origin).put("header", value)).toString()
    )
    try {
      assertTrue(write(header))
      val info = CookieManagerCompat.getCookieInfo(CookieManager.getInstance(), origin)
        .single { it.startsWith("szu_info_fixture=") }
      assertTrue(info.contains("Secure", ignoreCase = true))
      assertTrue(info.contains("HttpOnly", ignoreCase = true))
      assertTrue(info.contains("SameSite=Lax", ignoreCase = true))
    } finally {
      assertTrue(write("$header; Max-Age=0"))
    }
  }

  @Test fun modernReadRestoreAndCleanupWorkWithDevToolsDisabled() {
    assumeTrue("GET_COOKIE_INFO is unavailable on this WebView", WebviewCookieBridge.supportsCookieInfo())
    InstrumentationRegistry.getInstrumentation().runOnMainSync {
      android.webkit.WebView.setWebContentsDebuggingEnabled(false)
    }
    val origin = "https://modern-cookie-lifecycle-test.invalid/"
    val header = "szu_lifecycle_fixture=virtual; Path=/; Secure; HttpOnly; SameSite=Lax"
    fun write(value: String): Boolean = WebviewCookieBridge.writeCookies(
      JSONArray().put(JSONObject().put("url", origin).put("header", value)).toString()
    )
    fun read(): List<String> {
      val response = JSONObject(WebviewCookieBridge.readCookieInfo(JSONArray().put(origin).toString()))
      assertEquals("ok", response.getString("status"))
      val cookies = response.getJSONArray("cookies").getJSONArray(0)
      return (0 until cookies.length()).map { cookies.getString(it) }.filter { it.startsWith("szu_lifecycle_fixture=") }
    }
    try {
      assertTrue(write(header))
      val saved = read().single()
      assertTrue(write("$header; Max-Age=0"))
      assertTrue(read().isEmpty())
      // This is a host-only fixture; omit CookieInfo's canonical Domain on restore.
      assertTrue(write(header))
      assertEquals(saved, read().single())
      assertTrue(write("$header; Max-Age=0"))
      assertTrue(read().isEmpty())
    } finally {
      assertTrue(write("$header; Max-Age=0"))
      WebviewCookieBridge.bindContext(context)
      assertTrue(WebviewCookieBridge.prepareDevTools())
    }
  }

  @Test fun malformedWritesReturnFailureWithoutAnExceptionPayload() {
    assertFalse(WebviewCookieBridge.writeCookies("[{}]"))
  }
}
