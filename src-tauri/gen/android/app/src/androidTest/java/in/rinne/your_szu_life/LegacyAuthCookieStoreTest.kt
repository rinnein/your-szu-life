package `in`.rinne.your_szu_life

import android.webkit.CookieManager
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
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
      assertTrue(store.available())
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
}
