package `in`.rinne.your_szu_life

internal fun browserTitle(title: String?): String = title?.trim()?.takeIf { it.isNotEmpty() } ?: "泥深"

/**
 * Tauri 本地页面用 Web 顶栏，不再挂原生浏览器栏；只有受控打开的外部页面
 * （统一认证、自助服务、官方网费等）才需要原生栏提供标题与返回入口。
 */
internal fun shouldShowBrowserToolbar(localDocument: Boolean): Boolean = !localDocument

/** Web 端推来的系统栏颜色（CSS 颜色字符串，形如 `#rrggbb`）。 */
internal const val CHROME_STATUS_BAR_PREFIX = "chrome-status-bar:"
internal const val CHROME_NAVIGATION_BAR_PREFIX = "chrome-nav-bar:"

/**
 * 状态栏取「顶部条」当前的颜色。
 *
 * 外部页面的顶部条是原生浏览器栏（取主题色）；本地页面是 Web 顶栏，它滚动到一定
 * 位置会从 surface 变成 surface-container，颜色由 Web 端随滚动推送，这里只是取最新值。
 */
internal fun statusBarColorFor(
  toolbarVisible: Boolean,
  toolbarColor: Int,
  pushedColor: Int?,
  themeFallback: Int,
): Int = if (toolbarVisible) toolbarColor else pushedColor ?: themeFallback

/** 小白条固定取底部导航栏的颜色，不随滚动变化。 */
internal fun navigationBarColorFor(pushedColor: Int?, themeFallback: Int): Int =
  pushedColor ?: themeFallback

/**
 * 解析 Web 端推来的 CSS 颜色。
 *
 * 主题 token 一律是 3 位或 6 位十六进制；解析不出来时返回 null，
 * 调用方退回 Material 主题的 colorSurface，而不是把系统栏刷成黑色。
 */
internal fun parseCssColor(value: String?): Int? {
  val text = value?.trim()?.lowercase()?.takeIf { it.isNotEmpty() } ?: return null
  if (!text.startsWith("#")) return null
  val hex = text.substring(1)
  if (hex.any { it !in '0'..'9' && it !in 'a'..'f' }) return null
  val rgb = when (hex.length) {
    3 -> {
      val r = hex[0].digitToInt(16) * 17
      val g = hex[1].digitToInt(16) * 17
      val b = hex[2].digitToInt(16) * 17
      (r shl 16) or (g shl 8) or b
    }
    6 -> hex.toInt(16)
    else -> return null
  }
  return 0xFF000000.toInt() or rgb
}

/** sRGB 相对亮度（WCAG 口径），用于判断该底色上应当用深色还是浅色系统图标。 */
internal fun relativeLuminance(argb: Int): Double {
  fun channel(value: Int): Double {
    val v = value / 255.0
    return if (v <= 0.03928) v / 12.92 else Math.pow((v + 0.055) / 1.055, 2.4)
  }
  val r = channel((argb shr 16) and 0xFF)
  val g = channel((argb shr 8) and 0xFF)
  val b = channel(argb and 0xFF)
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

/** 浅色底上系统栏图标取深色（`isAppearanceLightStatusBars = true`）。 */
internal fun isLightSurface(argb: Int): Boolean = relativeLuminance(argb) > 0.5
