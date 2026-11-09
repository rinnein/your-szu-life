package `in`.rinne.your_szu_life

import android.content.Context
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteDatabaseLockedException
import android.database.sqlite.SQLiteTableLockedException
import android.os.Looper
import android.webkit.CookieManager
import java.io.File
import java.net.URI

/** Only attributes from this application's schema-14 store; never value/encrypted_value. */
internal class LegacyAuthCookieStore(private val context: Context) {
  private fun databaseFile(): File? {
    val candidates = listOf("app_webview", "app_hws_webview").flatMap { directory ->
      val root = File(context.dataDir, directory)
      listOf(File(root, "Cookies"), File(root, "Default/Cookies"), File(root, "Default/Network/Cookies"))
    }.filter { it.isFile }
    if (candidates.size > 1) throw CookieReadException(CookieReadError.UNSUPPORTED_STORE)
    val file = candidates.singleOrNull() ?: return null
    // Do not follow a symlink outside the application's private directory.
    if (!file.canonicalPath.startsWith(context.dataDir.canonicalPath + File.separator)) {
      throw CookieReadException(CookieReadError.UNSUPPORTED_STORE)
    }
    return file
  }

  private fun metadata(): List<LegacyCookieMetadata> {
    val file = databaseFile() ?: return emptyList()
    try {
      SQLiteDatabase.openDatabase(file.path, null, SQLiteDatabase.OPEN_READONLY).use { db ->
        val version = db.rawQuery("SELECT value FROM meta WHERE key = 'version'", null).use { cursor ->
          if (!cursor.moveToFirst()) throw CookieReadException(CookieReadError.UNSUPPORTED_STORE)
          cursor.getInt(0)
        }
        val columns = mutableSetOf<String>()
        db.rawQuery("PRAGMA table_info(cookies)", null).use { cursor ->
          while (cursor.moveToNext()) columns.add(cursor.getString(1))
        }
        validateLegacyCookieSchema(version, columns)
        val sameParty = if ("is_same_party" in columns) ", is_same_party" else ", 0"
        val rows = mutableListOf<LegacyCookieMetadata>()
        val now = System.currentTimeMillis()
        db.rawQuery(
          "SELECT name,host_key,path,expires_utc,has_expires,is_secure,is_httponly,samesite$sameParty " +
            "FROM cookies WHERE host_key IN (?,?,?) LIMIT 1025",
          arrayOf("authserver.szu.edu.cn", ".authserver.szu.edu.cn", ".szu.edu.cn"),
        ).use { cursor ->
          var count = 0
          while (cursor.moveToNext()) {
            if (++count > 1024) throw CookieReadException(CookieReadError.UNSUPPORTED_STORE)
            val row = LegacyCookieMetadata(
              cursor.getString(0), cursor.getString(1), cursor.getString(2), cursor.getLong(3),
              cursor.getInt(4) != 0, cursor.getInt(5) != 0, cursor.getInt(6) != 0, cursor.getInt(7),
            )
            if (row.inAuthScope() && !row.expired(now)) {
              if ((4..6).any { cursor.getInt(it) !in 0..1 } || cursor.getInt(8) != 0) {
                throw CookieReadException(CookieReadError.INVALID_METADATA)
              }
              rows.add(row)
            }
          }
        }
        return rows
      }
    } catch (_: SQLiteDatabaseLockedException) {
      throw CookieReadException(CookieReadError.SNAPSHOT_UNAVAILABLE)
    } catch (_: SQLiteTableLockedException) {
      throw CookieReadException(CookieReadError.SNAPSHOT_UNAVAILABLE)
    }
  }

  fun read(urls: List<URI>): List<String> {
    if (Looper.myLooper() == Looper.getMainLooper()) throw CookieReadException(CookieReadError.NATIVE_FAILURE)
    validateLegacyCookieUrls(urls)
    val manager = CookieManager.getInstance()
    // Force lazy initialization before locating the database. An absent store is
    // accepted only when both header observations confirm an empty auth jar.
    manager.hasCookies()
    return readStableLegacyCookies(urls, ::metadata, manager::getCookie, manager::flush)
  }
}
