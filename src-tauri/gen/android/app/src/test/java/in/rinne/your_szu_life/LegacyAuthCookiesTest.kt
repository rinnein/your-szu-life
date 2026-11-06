package `in`.rinne.your_szu_life

import org.junit.Assert.*
import org.junit.Test
import java.net.URI

class LegacyAuthCookiesTest {
  private val host = "authserver.szu.edu.cn"
  private val urls = listOf(URI("https://$host/authserver/login"), URI("https://$host/personalInfo/common/getUserConf"))
  private fun row(name: String = "SESSION", domain: String = host, path: String = "/") =
    LegacyCookieMetadata(name, domain, path, 0, false, true, true, -1)

  // 模拟 CookieManager，不假设同名 Cookie 的返回顺序。
  private fun header(cookies: List<Pair<LegacyCookieMetadata, String>>, url: String): String =
    cookies.reversed().filter { it.first.appliesTo(URI(url)) }
      .joinToString("; ") { "${it.first.name}=${it.second}" }

  @Test fun preservesHostDomainPathAndHttpOnlyForSameNames() {
    val cookies = listOf(
      row(domain = ".szu.edu.cn") to "parent",
      row(domain = ".$host") to "domain",
      row() to "host",
      row(path = "/authserver") to "login",
      row(path = "/personalInfo") to "profile",
    )
    val result = resolveLegacyCookieValues(cookies.map { it.first }.shuffled(), urls) { header(cookies, it) }
    assertEquals(5, result.size)
    assertTrue(result.contains("SESSION=host; Domain=$host; Path=/; Secure; HttpOnly"))
    assertTrue(result.contains("SESSION=domain; Domain=.$host; Path=/; Secure; HttpOnly"))
    assertTrue(result.contains("SESSION=login; Domain=$host; Path=/authserver; Secure; HttpOnly"))
    assertTrue(result.contains("SESSION=profile; Domain=$host; Path=/personalInfo; Secure; HttpOnly"))
  }

  @Test fun retainsEqualValuesAsDistinctCookies() {
    val cookies = listOf(row(domain = ".$host") to "same", row() to "same", row(path = "/authserver") to "same")
    assertEquals(3, resolveLegacyCookieValues(cookies.map { it.first }, urls) { header(cookies, it) }.size)
  }

  @Test fun rejectsMissingMetadataAndStaleValues() {
    val captured = row()
    assertThrows(IllegalStateException::class.java) {
      resolveLegacyCookieValues(listOf(captured), urls) { "SESSION=one; SESSION=unrecorded" }
    }
    assertThrows(IllegalStateException::class.java) {
      resolveLegacyCookieValues(listOf(captured), urls) { url -> if (url.endsWith("/")) "SESSION=before" else "SESSION=after" }
    }
    assertThrows(IllegalStateException::class.java) {
      resolveLegacyCookieValues(emptyList(), urls) { "SESSION=missing" }
    }
  }

  @Test fun excludesForeignHostsAndUnrelatedPaths() {
    assertFalse(row(domain = "other.szu.edu.cn").inAuthScope())
    assertFalse(row(domain = "szu.edu.cn").inAuthScope())
    assertFalse(row(path = "/personalInformation").inAuthScope())
    assertFalse(legacyCookiePathMatches("/personalInfo", "/personalInformation"))
    assertTrue(legacyCookiePathMatches("/personalInfo", "/personalInfo/common/getUserConf"))
    assertFalse(row().appliesTo(URI("https://child.$host/")))
    assertTrue(row(domain = ".$host").appliesTo(URI("https://child.$host/")))
    assertFalse(row().appliesTo(URI("http://$host/")))
  }

  @Test fun convertsWindowsExpiryAndSameSiteWithoutInventingSessionExpiry() {
    val session = row()
    assertFalse(session.canonical("opaque").contains("Expires"))
    val persistent = session.copy(expiresUtc = 11644473600000000L + 60000000L, hasExpires = true, sameSite = 1)
    assertFalse(persistent.expired(59999))
    assertTrue(persistent.expired(60000))
    assertTrue(persistent.canonical("opaque").contains("Expires=Thu, 01 Jan 1970 00:01:00 +0000"))
    assertTrue(persistent.canonical("opaque").contains("SameSite=Lax"))
    assertThrows(IllegalStateException::class.java) { session.copy(sameSite = 7).canonical("opaque") }
  }

  @Test fun rejectsDuplicateScopesAndInvalidHeaderValues() {
    assertThrows(IllegalStateException::class.java) {
      resolveLegacyCookieValues(listOf(row(), row()), urls) { "SESSION=value" }
    }
    assertThrows(IllegalStateException::class.java) { row(name = "bad;name").canonical("opaque") }
    assertThrows(IllegalStateException::class.java) { row().canonical("value\r\nInjected: true") }
  }
}
