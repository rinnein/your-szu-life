package `in`.rinne.your_szu_life

import android.content.Context
import android.database.sqlite.SQLiteDatabase
import android.webkit.CookieManager
import java.io.File
import java.net.URI
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

private const val AUTH_COOKIE_HOST = "authserver.szu.edu.cn"
private const val WINDOWS_EPOCH_SECONDS = 11644473600L
private val AUTH_COOKIE_PATHS = listOf(
  "/authserver/login", "/personalInfo/personCenter/index.html",
  "/personalInfo/personalMobile/index.html", "/personalInfo/common/getUserConf",
)

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
    check(name.isNotEmpty() && !name.any { it == '=' || it == ';' || it.isISOControl() }) {
      "cookie-name-invalid"
    }
    check(!value.any { it == ';' || it.isISOControl() }) { "cookie-value-invalid" }
    val sameSiteName = when (sameSite) {
      -1 -> null
      0 -> "None"
      1 -> "Lax"
      2 -> "Strict"
      else -> error("cookie-samesite-unsupported")
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

private fun requestCookiePairs(header: String?): MutableList<Pair<String, String>> =
  header.orEmpty().split(';').filter { it.isNotBlank() }.map { part ->
    val pair = part.trim().split('=', limit = 2)
    check(pair.size == 2 && pair[0].isNotEmpty()) { "cookie-header-invalid" }
    pair[0] to pair[1]
  }.toMutableList()

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
  check(rows.distinctBy { Triple(it.name, it.hostKey, it.path) }.size == rows.size) {
    "cookie-metadata-duplicate"
  }
  val resolved = linkedMapOf<LegacyCookieMetadata, String>()
  // 父域先于子域、Domain 先于 host-only、父路径先于子路径。
  val ordered = rows.sortedWith(compareBy<LegacyCookieMetadata>(
    { it.domain.count { c -> c == '.' } }, { it.hostOnly }, { it.path.length },
  ))
  for (row in ordered) {
    val probeHost = if (row.hostOnly) row.domain else "szu-cookie-scope-probe.${row.domain}"
    val probe = URI("https", null, probeHost, -1, row.path, null, null)
    val candidates = rows.filter { it.name == row.name && it.appliesTo(probe) && it !in resolved }
    check(candidates.size == 1 && candidates[0] == row) { "cookie-scope-ambiguous" }
    val values = requestCookiePairs(readHeader(probe.toASCIIString()))
      .filter { it.first == row.name }.map { it.second }.toMutableList()
    for ((known, value) in resolved) {
      if (known.name == row.name && known.appliesTo(probe)) {
        check(values.remove(value)) { "cookie-metadata-stale" }
      }
    }
    check(values.size == 1) { "cookie-value-unavailable" }
    resolved[row] = values.single()
  }
  // flush 后的磁盘快照仍可能落后于内核；返回前逐 URL 验证 Cookie 名称和值完全一致。
  for (url in urls) {
    val expected = resolved.filterKeys { it.appliesTo(url) }.map { it.key.name to it.value }
      .groupingBy { it }.eachCount()
    val current = requestCookiePairs(readHeader(url.toASCIIString())).groupingBy { it }.eachCount()
    check(expected == current) { "cookie-metadata-stale" }
  }
  return resolved.map { (row, value) -> row.canonical(value) }
}

/**
 * GET_COOKIE_INFO 不可用时，只读当前应用自己的旧 Chromium Cookie 属性。
 * 仅支持已知 WebView 目录和旧版 schema；路径、字段、SameSite 或快照不一致时拒绝降级。
 * 不读取 value/encrypted_value 列，不解密，不修改 Cookie 数据库。
 */
internal class LegacyAuthCookieStore(private val context: Context) {
  private fun databaseFile(): File? {
    val roots = listOf("app_webview", "app_hws_webview").map { File(context.dataDir, it) }
    val candidates = roots.flatMap { root ->
      listOf(File(root, "Cookies"), File(root, "Default/Cookies"), File(root, "Default/Network/Cookies"))
    }.filter { it.isFile }
    check(candidates.size <= 1) { "cookie-store-ambiguous" }
    return candidates.singleOrNull()
  }

  private fun metadata(manager: CookieManager): List<LegacyCookieMetadata> {
    manager.flush()
    val file = databaseFile()
    if (file == null) {
      check(!manager.hasCookies()) { "cookie-store-unavailable" }
      return emptyList()
    }
    SQLiteDatabase.openDatabase(file.path, null, SQLiteDatabase.OPEN_READONLY).use { db ->
      // 华为 Chrome 92 使用 Chromium schema 14；其它私有格式必须单独验证后再支持。
      db.rawQuery("SELECT value FROM meta WHERE key = 'version'", null).use { cursor ->
        check(cursor.moveToFirst() && cursor.getInt(0) == 14) { "cookie-schema-unsupported" }
      }
      val columns = mutableSetOf<String>()
      db.rawQuery("PRAGMA table_info(cookies)", null).use { cursor ->
        while (cursor.moveToNext()) columns.add(cursor.getString(1))
      }
      val required = setOf("name", "host_key", "path", "expires_utc", "has_expires", "is_secure", "is_httponly", "samesite")
      check(columns.containsAll(required)) { "cookie-schema-unsupported" }
      // 新版 partitioned/same-party Cookie 不能被旧格式无损表示。
      val sameParty = if ("is_same_party" in columns) ", is_same_party" else ", 0"
      check("top_frame_site_key" !in columns && "is_partitioned" !in columns) {
        "cookie-schema-unsupported"
      }
      val rows = mutableListOf<LegacyCookieMetadata>()
      db.rawQuery(
        "SELECT name,host_key,path,expires_utc,has_expires,is_secure,is_httponly,samesite$sameParty " +
          "FROM cookies WHERE host_key IN (?,?,?) LIMIT 1025",
        arrayOf(AUTH_COOKIE_HOST, ".$AUTH_COOKIE_HOST", ".szu.edu.cn"),
      ).use { cursor ->
        while (cursor.moveToNext()) {
          check(rows.size < 1024) { "cookie-store-too-large" }
          val row = LegacyCookieMetadata(
            cursor.getString(0), cursor.getString(1), cursor.getString(2), cursor.getLong(3),
            cursor.getInt(4) != 0, cursor.getInt(5) != 0, cursor.getInt(6) != 0, cursor.getInt(7),
          )
          if (row.inAuthScope() && !row.expired(System.currentTimeMillis())) {
            check(cursor.getInt(8) == 0) { "cookie-same-party-unsupported" }
            rows.add(row)
          }
        }
      }
      return rows
    }
  }

  fun available(): Boolean = runCatching {
    read(AUTH_COOKIE_PATHS.map { URI("https", null, AUTH_COOKIE_HOST, -1, it, null, null) })
    true
  }.getOrDefault(false)

  fun read(urls: List<URI>): List<String> {
    check(urls.isNotEmpty() && urls.all {
      it.scheme == "https" && it.host == AUTH_COOKIE_HOST && it.userInfo == null &&
        it.port in setOf(-1, 443) && it.path in AUTH_COOKIE_PATHS
    }) { "cookie-url-out-of-scope" }
    val manager = CookieManager.getInstance()
    return resolveLegacyCookieValues(metadata(manager), urls, manager::getCookie)
  }
}
