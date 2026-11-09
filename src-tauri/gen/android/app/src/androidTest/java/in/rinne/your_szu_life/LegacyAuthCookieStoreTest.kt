package `in`.rinne.your_szu_life

import android.webkit.CookieManager
import android.content.ContextWrapper
import android.database.sqlite.SQLiteDatabase
import androidx.webkit.WebViewFeature
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.Assume.*
import org.json.JSONObject
import java.io.File
import java.net.URI
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class LegacyAuthCookieStoreTest {
  private val origin = "https://authserver.szu.edu.cn"
  private val name = "szu_compat_roundtrip"
  private val instrumentation = InstrumentationRegistry.getInstrumentation()
  private fun write(header: String) {
    val done = CountDownLatch(1)
    instrumentation.runOnMainSync {
      CookieManager.getInstance().setCookie("$origin/", header) { accepted ->
        assertTrue(accepted); done.countDown()
      }
    }
    assertTrue(done.await(5, TimeUnit.SECONDS))
    CookieManager.getInstance().flush()
  }

  @Test fun legacyCookiesRetainAttributesAndCanBeRestored() {
    assumeFalse(WebViewFeature.isFeatureSupported(WebViewFeature.GET_COOKIE_INFO))
    val context = instrumentation.targetContext
    val file = listOf("app_webview", "app_hws_webview").flatMap { directory ->
      listOf("Cookies", "Default/Cookies", "Default/Network/Cookies").map { File(context.dataDir, "$directory/$it") }
    }.singleOrNull { it.isFile }
    assumeNotNull(file)
    val schema = SQLiteDatabase.openDatabase(file!!.path, null, SQLiteDatabase.OPEN_READONLY).use { db ->
      db.rawQuery("SELECT value FROM meta WHERE key='version'", null).use { cursor ->
        if (cursor.moveToFirst()) cursor.getInt(0) else -1
      }
    }
    assumeTrue("requires a schema-14 WebView", schema == 14)
    val store = LegacyAuthCookieStore(instrumentation.targetContext)
    val urls = listOf(URI("$origin/authserver/login"), URI("$origin/personalInfo/common/getUserConf"))
    val definitions = listOf(
      "$name=domain-test; Domain=authserver.szu.edu.cn; Path=/; Secure; HttpOnly; SameSite=Lax",
      "$name=host-test; Path=/; Secure; HttpOnly; SameSite=Lax",
      "$name=login-test; Path=/authserver; Secure; HttpOnly",
      "$name=profile-test; Path=/personalInfo; Secure; HttpOnly",
    )
    fun remove() {
      for (header in definitions) write(header + "; Max-Age=0")
    }
    try {
      remove()
      definitions.forEach(::write)
      val captured = store.read(urls).filter { it.startsWith("$name=") }
      assertEquals(4, captured.size)
      assertTrue(captured.any { it.startsWith("$name=host-test;") && it.contains("Domain=authserver.szu.edu.cn") && it.contains("HttpOnly") })
      assertTrue(captured.any { it.startsWith("$name=domain-test;") && it.contains("Domain=.authserver.szu.edu.cn") && it.contains("SameSite=Lax") })
      assertTrue(captured.any { it.startsWith("$name=login-test;") && it.contains("Path=/authserver") })
      assertTrue(captured.any { it.startsWith("$name=profile-test;") && it.contains("Path=/personalInfo") })
      remove()
      assertEquals(0, store.read(urls).count { it.startsWith("$name=") })
      // canonical 输出中的 Domain 是元数据编码；host-only 还原时省略 Domain 属性。
      captured.forEach { cookie ->
        val restored = cookie.replace("; Domain=authserver.szu.edu.cn", "")
        write(restored)
      }
      assertEquals(captured.toSet(), store.read(urls).filter { it.startsWith("$name=") }.toSet())
    } finally {
      remove()
    }
  }

  @Test fun modernCookieApiUsesTheSameNativeEnvelope() {
    assumeTrue(WebViewFeature.isFeatureSupported(WebViewFeature.GET_COOKIE_INFO))
    AuthCookieBridge.bindContext(instrumentation.targetContext)
    val definition = "$name=modern-test; Path=/; Secure; HttpOnly; SameSite=Lax"
    try {
      write(definition)
      val result = JSONObject(AuthCookieBridge.readCookies("[\"$origin/authserver/login\"]"))
      assertEquals("ok", result.getString("status"))
      val cookies = result.getJSONArray("cookies")
      val captured = (0 until cookies.length()).map { cookies.getString(it) }.filter { it.startsWith("$name=") }
      assertEquals(1, captured.size)
      assertTrue(captured.single().contains("modern-test"))
      assertTrue(captured.single().lowercase().contains("httponly"))
    } finally {
      write(definition + "; Max-Age=0")
    }
  }

  @Test fun rejectsUnknownSchemaInAnIsolatedDatabase() {
    val context = instrumentation.targetContext
    val directory = File(context.cacheDir, "cookie-schema-test-${System.nanoTime()}")
    val testContext = object : ContextWrapper(context) {
      override fun getDataDir(): File = directory
    }
    val database = File(directory, "app_webview/Cookies")
    database.parentFile!!.mkdirs()
    try {
      SQLiteDatabase.openOrCreateDatabase(database, null).use { db ->
        db.execSQL("CREATE TABLE meta (key TEXT, value INTEGER)")
        db.execSQL("INSERT INTO meta VALUES ('version', 24)")
        db.execSQL("CREATE TABLE cookies (name TEXT)")
      }
      val failure = assertThrows(CookieReadException::class.java) {
        LegacyAuthCookieStore(testContext).read(listOf(URI("$origin/authserver/login")))
      }
      assertEquals(CookieReadError.UNSUPPORTED_STORE, failure.code)
    } finally {
      directory.deleteRecursively()
    }
  }
}
