package `in`.rinne.your_szu_life

import android.app.WallpaperManager
import android.content.Context
import android.content.res.Configuration
import android.os.Build
import android.util.Log

/**
 * Reads the wallpaper-derived seed colour so the WebView can build the same
 * Material You palette Android uses for its own chrome.
 *
 * WallpaperManager already quantises the wallpaper and returns its primary
 * colour, so no client-side quantisation is needed. Reusing the system's
 * choice also guarantees the WebView and the native toolbar agree.
 */
object WallpaperColorBridge {
  private const val TAG = "WallpaperColorBridge"

  @Volatile private var applicationContext: Context? = null

  /** Last seed handed to Rust, so a change can be detected on resume. */
  @Volatile private var lastSeed: Int? = null

  @JvmStatic
  fun bindActivity(activity: android.app.Activity) {
    applicationContext = activity.applicationContext
  }

  @JvmStatic
  fun unbindActivity() {
    applicationContext = null
  }

  @JvmStatic
  fun getSeedColorJson(): String {
    val context = applicationContext ?: return seedJson(null, "unavailable", false)
    val dark = (context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) ==
        Configuration.UI_MODE_NIGHT_YES
    val seed = readSeed(context)
    lastSeed = seed
    return seedJson(seed, source(context, seed), dark)
  }

  /**
   * Wallpaper changes emit no broadcast we can subscribe to, so this is polled
   * from onResume. Returns true when the seed moved since the previous read.
   */
  @JvmStatic
  fun hasSeedChanged(): Boolean {
    val context = applicationContext ?: return false
    val current = readSeed(context)
    if (current == lastSeed) return false
    lastSeed = current
    return true
  }

  private fun readSeed(context: Context): Int? =
    wallpaperSeed(context) ?: systemAccentSeed(context)

  private fun wallpaperSeed(context: Context): Int? = try {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
      val manager = context.getSystemService(Context.WALLPAPER_SERVICE) as? WallpaperManager
      manager?.getWallpaperColors(WallpaperManager.FLAG_SYSTEM)?.primaryColor?.toArgb()
    } else {
      null
    }
  } catch (error: Throwable) {
    Log.w(TAG, "wallpaper colour unavailable", error)
    null
  }

  /** Android 12+ exposes the wallpaper-derived accent even when reading the wallpaper is blocked. */
  private fun systemAccentSeed(context: Context): Int? = try {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
      context.getColor(android.R.color.system_accent1_500)
    } else {
      null
    }
  } catch (error: Throwable) {
    Log.w(TAG, "system accent unavailable", error)
    null
  }

  private fun source(context: Context, seed: Int?): String = when {
    seed == null -> "unavailable"
    wallpaperSeed(context) != null -> "wallpaper"
    else -> "system-accent"
  }
}

/**
 * Pure helpers, unit-tested without an emulator (see WallpaperColorBridgeTest).
 *
 * `seed` is an ARGB int; `null` means the platform exposed no wallpaper or
 * accent colour, in which case the WebView keeps its own default seed.
 */
internal fun seedJson(seed: Int?, source: String, dark: Boolean): String {
  val hex = seed?.let { formatHex(it) }
  val seedField = if (hex == null) "null" else "\"$hex\""
  return """{"seed":$seedField,"source":"$source","dark":$dark}"""
}

/** ARGB int -> `#rrggbb`, dropping alpha. */
internal fun formatHex(argb: Int): String {
  val rgb = argb and 0xFFFFFF
  return "#" + rgb.toString(16).padStart(6, '0')
}
