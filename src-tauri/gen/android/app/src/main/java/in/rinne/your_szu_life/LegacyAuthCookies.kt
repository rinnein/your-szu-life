package `in`.rinne.your_szu_life

import java.net.URI
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

private const val AUTH_COOKIE_HOST = "authserver.szu.edu.cn"
private const val WINDOWS_EPOCH_SECONDS = 11644473600L
internal val AUTH_COOKIE_PATHS = listOf(
  "/authserver/login", "/personalInfo/personCenter/index.html",
  "/personalInfo/personalMobile/index.html", "/personalInfo/common/getUserConf",
)

internal enum class CookieReadError(val wireName: String) {
  UNSUPPORTED_STORE("unsupportedStore"),
  INVALID_METADATA("invalidMetadata"),
  SNAPSHOT_UNAVAILABLE("snapshotUnavailable"),
  NATIVE_FAILURE("nativeFailure"),
}

/** Fixed categories only: exceptions crossing JNI must never contain Cookie data. */
internal class CookieReadException(val code: CookieReadError) : IllegalStateException(code.wireName)

private fun requireCookie(condition: Boolean, code: CookieReadError) {
  if (!condition) throw CookieReadException(code)
}

internal fun validateLegacyCookieSchema(version: Int, columns: Set<String>) {
  val required = setOf("name", "host_key", "path", "expires_utc", "has_expires", "is_secure", "is_httponly", "samesite")
  requireCookie(version == 14 && columns.containsAll(required) &&
    "top_frame_site_key" !in columns && "is_partitioned" !in columns,
    CookieReadError.UNSUPPORTED_STORE)
}

/** Chromium Cookie 元数据；Cookie 值始终由原生 CookieManager 获取，不解密或读取数据库值。 */
internal data class LegacyCookieMetadata(
  val name: String,
  val hostKey: String,
  val path: String,
  val expiresUtc: Long,
  val hasExpires: Boolean,
  val secure: Boolean,
  val httpOnly: Boolean,
  val sameSite: Int,
) {
  val hostOnly: Boolean get() = !hostKey.startsWith('.')
  val domain: String get() = hostKey.removePrefix(".")

  fun appliesTo(url: URI): Boolean =
    (url.host == domain || (!hostOnly && url.host.endsWith(".$domain"))) &&
      (!secure || url.scheme == "https") && legacyCookiePathMatches(path, url.path)

  fun inAuthScope(): Boolean =
    hostKey in setOf(AUTH_COOKIE_HOST, ".$AUTH_COOKIE_HOST", ".szu.edu.cn") &&
      path.startsWith('/') && !path.any { it == ';' || it.isISOControl() } &&
      AUTH_COOKIE_PATHS.any { legacyCookiePathMatches(path, it) }

  fun expired(nowMillis: Long): Boolean =
    hasExpires && expiresUtc / 1000 - WINDOWS_EPOCH_SECONDS * 1000 <= nowMillis

  fun canonical(value: String): String {
    requireCookie(name.isNotEmpty() && !name.any { it == '=' || it == ';' || it.isISOControl() },
      CookieReadError.INVALID_METADATA)
    requireCookie(!value.any { it == ';' || it.isISOControl() }, CookieReadError.INVALID_METADATA)
    val sameSiteName = when (sameSite) {
      -1 -> null
      0 -> "None"
      1 -> "Lax"
      2 -> "Strict"
      else -> throw CookieReadException(CookieReadError.INVALID_METADATA)
    }
    return buildString {
      append(name).append('=').append(value)
      // 与 CookieManagerCompat 的规范化格式一致：前导点保留 host/domain 区别。
      append("; Domain=").append(hostKey)
      append("; Path=").append(path)
      if (secure) append("; Secure")
      if (httpOnly) append("; HttpOnly")
      if (sameSiteName != null) append("; SameSite=").append(sameSiteName)
      if (hasExpires) {
        val format = SimpleDateFormat("EEE, dd MMM yyyy HH:mm:ss Z", Locale.US)
        format.timeZone = TimeZone.getTimeZone("GMT")
        append("; Expires=").append(format.format(Date(expiresUtc / 1000 - WINDOWS_EPOCH_SECONDS * 1000)))
      }
    }
  }
}

internal fun legacyCookiePathMatches(cookiePath: String, requestPath: String): Boolean =
  requestPath == cookiePath ||
    (requestPath.startsWith(cookiePath) &&
      (cookiePath.endsWith('/') || requestPath.getOrNull(cookiePath.length) == '/'))

private fun requestCookiePairs(header: String?): MutableList<Pair<String, String>> {
  requireCookie(!header.orEmpty().any { it.isISOControl() }, CookieReadError.INVALID_METADATA)
  return header.orEmpty().split(';').filter { it.isNotBlank() }.map { part ->
    val pair = part.trim().split('=', limit = 2)
    requireCookie(pair.size == 2 && pair[0].isNotEmpty(), CookieReadError.INVALID_METADATA)
    pair[0] to pair[1]
  }.toMutableList()
}

/**
 * 按作用域从宽到窄取值，不依赖 getCookie() 的同名 Cookie 排序。
 * Domain Cookie 通过虚拟子域读取（不发网络请求），排除同路径的 host-only Cookie；
 * 已知父域/父路径的值以多重集扣除，剩下必须恰好对应一个 Cookie。
 */
internal fun resolveLegacyCookieValues(
  metadata: List<LegacyCookieMetadata>,
  urls: List<URI>,
  readHeader: (String) -> String?,
): List<String> {
  val rows = metadata.filter { it.inAuthScope() }
  requireCookie(rows.distinctBy { Triple(it.name, it.hostKey, it.path) }.size == rows.size,
    CookieReadError.INVALID_METADATA)
  // Validate attributes before probing, so malformed metadata is never retried as a stale value.
  rows.forEach { it.canonical("") }
  val resolved = linkedMapOf<LegacyCookieMetadata, String>()
  // 父域先于子域、Domain 先于 host-only、父路径先于子路径。
  val ordered = rows.sortedWith(compareBy<LegacyCookieMetadata>(
    { it.domain.count { c -> c == '.' } }, { it.hostOnly }, { it.path.length },
  ))
  for (row in ordered) {
    val probeHost = if (row.hostOnly) row.domain else "szu-cookie-scope-probe.${row.domain}"
    val probe = URI("https", null, probeHost, -1, row.path, null, null)
    val candidates = rows.filter { it.name == row.name && it.appliesTo(probe) && it !in resolved }
    requireCookie(candidates.size == 1 && candidates[0] == row, CookieReadError.INVALID_METADATA)
    val values = requestCookiePairs(readHeader(probe.toASCIIString()))
      .filter { it.first == row.name }.map { it.second }.toMutableList()
    for ((known, value) in resolved) {
      if (known.name == row.name && known.appliesTo(probe)) {
        requireCookie(values.remove(value), CookieReadError.SNAPSHOT_UNAVAILABLE)
      }
    }
    requireCookie(values.size == 1, CookieReadError.SNAPSHOT_UNAVAILABLE)
    resolved[row] = values.single()
  }
  // flush 后的磁盘快照仍可能落后于内核；返回前逐 URL 验证 Cookie 名称和值完全一致。
  for (url in urls) {
    val expected = resolved.filterKeys { it.appliesTo(url) }.map { it.key.name to it.value }
      .groupingBy { it }.eachCount()
    val current = requestCookiePairs(readHeader(url.toASCIIString())).groupingBy { it }.eachCount()
    requireCookie(expected == current, CookieReadError.SNAPSHOT_UNAVAILABLE)
  }
  return resolved.map { (row, value) -> row.canonical(value) }
}

internal fun validateLegacyCookieUrls(urls: List<URI>) {
    requireCookie(urls.isNotEmpty() && urls.all {
      it.scheme == "https" && it.host == AUTH_COOKIE_HOST && it.userInfo == null &&
        it.port in setOf(-1, 443) && it.path in AUTH_COOKIE_PATHS
    }, CookieReadError.INVALID_METADATA)
}

/** A stable observation, not a lock on Chromium's independent network writes. */
internal fun readStableLegacyCookies(
  urls: List<URI>,
  readMetadata: () -> List<LegacyCookieMetadata>,
  readHeader: (String) -> String?,
  flush: () -> Unit,
  waitForRetry: (Long) -> Unit = { Thread.sleep(it) },
): List<String> {
  validateLegacyCookieUrls(urls)
  repeat(3) { attempt ->
    try {
      flush()
      val before = readMetadata()
      val captured = resolveLegacyCookieValues(before, urls, readHeader)
      flush()
      val after = readMetadata()
      requireCookie(before.toSet() == after.toSet(), CookieReadError.SNAPSHOT_UNAVAILABLE)
      // Also recheck values after the second disk observation.
      val verified = resolveLegacyCookieValues(after, urls, readHeader)
      requireCookie(captured.toSet() == verified.toSet(), CookieReadError.SNAPSHOT_UNAVAILABLE)
      return verified
    } catch (failure: CookieReadException) {
      if (failure.code != CookieReadError.SNAPSHOT_UNAVAILABLE || attempt == 2) throw failure
      waitForRetry(50)
    }
  }
  error("unreachable")
}
