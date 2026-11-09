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

  @Test fun retriesChangedAttributesEvenWhenCookieValueStaysTheSame() {
    val original = row()
    for (updated in listOf(
      original.copy(httpOnly = false), original.copy(secure = false), original.copy(sameSite = 1),
      original.copy(hasExpires = true, expiresUtc = 13300000000000000L),
    )) {
      var observations = 0
      val waits = mutableListOf<Long>()
      val result = readStableLegacyCookies(urls,
        { listOf(if (++observations == 1) original else updated) },
        { "SESSION=unchanged" }, {}, waits::add)
      assertEquals(4, observations)
      assertEquals(listOf(50L), waits)
      assertEquals(listOf(updated.canonical("unchanged")), result)
    }
  }

  @Test fun retriesValueChangesAfterTheSecondMetadataObservation() {
    var flushes = 0
    val waits = mutableListOf<Long>()
    val result = readStableLegacyCookies(urls, { listOf(row()) },
      { if (flushes == 1) "SESSION=before" else "SESSION=after" },
      { flushes++ }, waits::add)
    assertEquals(listOf(50L), waits)
    assertEquals(listOf(row().canonical("after")), result)
  }

  @Test fun limitsUnstableSnapshotsToThreeAttempts() {
    var observations = 0
    val waits = mutableListOf<Long>()
    val failure = assertThrows(CookieReadException::class.java) {
      readStableLegacyCookies(urls, { listOf(row().copy(httpOnly = ++observations % 2 == 0)) },
        { "SESSION=same" }, {}, waits::add)
    }
    assertEquals(CookieReadError.SNAPSHOT_UNAVAILABLE, failure.code)
    assertEquals(6, observations)
    assertEquals(listOf(50L, 50L), waits)
  }

  @Test fun retriesTemporaryDatabaseLocksWithoutSleepingInTests() {
    var observations = 0
    val waits = mutableListOf<Long>()
    val result = readStableLegacyCookies(urls, {
      if (++observations < 3) throw CookieReadException(CookieReadError.SNAPSHOT_UNAVAILABLE)
      listOf(row())
    }, { "SESSION=ready" }, {}, waits::add)
    assertEquals(listOf(50L, 50L), waits)
    assertEquals(listOf(row().canonical("ready")), result)
  }

  @Test fun acceptsAbsentMetadataOnlyForAnEmptyAuthJar() {
    assertEquals(emptyList<String>(), readStableLegacyCookies(urls, { emptyList() }, { null }, {}))
    val waits = mutableListOf<Long>()
    val failure = assertThrows(CookieReadException::class.java) {
      readStableLegacyCookies(urls, { emptyList() }, { "SESSION=unrecorded" }, {}, waits::add)
    }
    assertEquals(CookieReadError.SNAPSHOT_UNAVAILABLE, failure.code)
    assertEquals(listOf(50L, 50L), waits)
  }

  @Test fun doesNotRetryUnknownSchemasMissingColumnsOrInvalidMetadata() {
    val columns = setOf("name", "host_key", "path", "expires_utc", "has_expires", "is_secure", "is_httponly", "samesite")
    validateLegacyCookieSchema(14, columns)
    for ((version, fields) in listOf(24 to columns, 14 to (columns - "path"), 14 to (columns + "top_frame_site_key"))) {
      val waits = mutableListOf<Long>()
      var observations = 0
      val failure = assertThrows(CookieReadException::class.java) {
        readStableLegacyCookies(urls, {
          observations++
          validateLegacyCookieSchema(version, fields)
          emptyList()
        }, { null }, {}, waits::add)
      }
      assertEquals(CookieReadError.UNSUPPORTED_STORE, failure.code)
      assertEquals(1, observations)
      assertTrue(waits.isEmpty())
    }
    val failure = assertThrows(CookieReadException::class.java) {
      readStableLegacyCookies(urls, { listOf(row().copy(sameSite = 7)) }, { "SESSION=opaque" }, {},
        { fail("invalid attributes must not be retried") })
    }
    assertEquals(CookieReadError.INVALID_METADATA, failure.code)
  }

  @Test fun rejectsForeignUrlsBeforeReadingAnyData() {
    for (requested in listOf(emptyList(), listOf(URI("http://$host/authserver/login")),
      listOf(URI("https://other.szu.edu.cn/authserver/login")), listOf(URI("https://$host:444/authserver/login")),
      listOf(URI("https://user@$host/authserver/login")), listOf(URI("https://$host/unrelated")))) {
      val failure = assertThrows(CookieReadException::class.java) {
        readStableLegacyCookies(requested, { fail("metadata must not be read"); emptyList() },
          { fail("header must not be read"); null }, {})
      }
      assertEquals(CookieReadError.INVALID_METADATA, failure.code)
    }
  }

  @Test fun rejectsControlCharactersInNativeHeadersWithoutRetrying() {
    val failure = assertThrows(CookieReadException::class.java) {
      readStableLegacyCookies(urls, { listOf(row()) }, { "SESSION=value\r\n" }, {},
        { fail("invalid headers must not be retried") })
    }
    assertEquals(CookieReadError.INVALID_METADATA, failure.code)
  }

  @Test fun doesNotPersistExpiredCookiesAndKeepsSessionExpiryAbsent() {
    val expired = row().copy(hasExpires = true, expiresUtc = 11644473600000000L + 60000000L)
    val metadata = listOf(row(), expired.copy(name = "OLD")).filter { !it.expired(60000) }
    val result = readStableLegacyCookies(urls, { metadata }, { "SESSION=live" }, {})
    assertEquals(listOf(row().canonical("live")), result)
    assertFalse(result.single().contains("Expires="))
  }
}
