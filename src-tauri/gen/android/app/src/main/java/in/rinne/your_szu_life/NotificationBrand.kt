package `in`.rinne.your_szu_life

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import androidx.core.app.NotificationCompat
import com.google.android.material.color.DynamicColors
import com.google.android.material.color.MaterialColors

/**
 * 上课提醒通知的「品牌外观」：logo、强调色与进度条配色。
 *
 * ColorOS 流体云读取的是标准 promoted 通知，卡片背景由系统按自身主题绘制；
 * 应用能决定的只有 large icon、通知强调色（`setColor`）与 `ProgressStyle` 的段色。
 * 这三项都从这里出，实时活动与定点回退通知共用同一套外观，避免两条路径分叉。
 */
object NotificationBrand {
  private const val PREFS = "notification_palette"
  private const val KEY_ACCENT = "accent"

  /** 已解码的 logo：实时活动每 15 秒重发一次通知，不能每次都解码一遍。 */
  @Volatile
  private var cachedLargeIcon: Bitmap? = null

  /**
   * 落盘 Web 端推来的强调色（M3 primary）。
   *
   * 必须持久化：闹钟可能在应用进程被系统回收后才触发，那时没有 WebView 可以取色。
   * `null` 表示解析失败或没有推过，清除后读取时退回主题色。
   */
  fun saveAccent(context: Context, accent: Int?) {
    val editor = prefs(context).edit()
    if (accent == null) editor.remove(KEY_ACCENT) else editor.putInt(KEY_ACCENT, accent)
    editor.apply()
  }

  /**
   * 通知强调色：优先用 Web 端推来并落盘的值（与 WebView 内的 M3 primary 完全一致，
   * 包括用户在应用里强制的深浅模式）；还没收到推送时退回壁纸派生的主题 primary。
   */
  fun accent(context: Context): Int? {
    val stored = prefs(context).getInt(KEY_ACCENT, 0)
    if (stored != 0) return stored
    return themeAccent(context)
  }

  /** 进度条配色：单段 = 100，已填充部分用强调色，未填充部分由系统自动减淡。 */
  fun progressStyle(progress: Int, accent: Int?): NotificationCompat.ProgressStyle {
    val style = NotificationCompat.ProgressStyle()
      .setProgress(progress)
      .setStyledByProgress(true)
    if (accent != null) {
      style.addProgressSegment(NotificationCompat.ProgressStyle.Segment(100).setColor(accent))
    }
    return style
  }

  /** 统一的卡片外观：右上角 logo + 强调色。 */
  fun apply(
    builder: NotificationCompat.Builder,
    context: Context,
    accent: Int?,
  ): NotificationCompat.Builder {
    largeIcon(context)?.let { builder.setLargeIcon(it) }
    if (accent != null) builder.setColor(accent)
    return builder
  }

  /** 应用 logo（与启动图标同一套素材）；解码失败时返回 null，卡片不出图。 */
  fun largeIcon(context: Context): Bitmap? {
    cachedLargeIcon?.let { return it }
    return runCatching {
      BitmapFactory.decodeResource(context.resources, R.mipmap.ic_launcher)
    }.getOrNull()?.also { cachedLargeIcon = it }
  }

  private fun themeAccent(context: Context): Int? = runCatching {
    // 必须 wrap：DynamicColors.applyToActivityIfAvailable 只处理 Activity，
    // Service / BroadcastReceiver 的 context 会退回基线主题色（不是壁纸色）。
    val themed = DynamicColors.wrapContextIfAvailable(context)
    MaterialColors.getColor(
      themed,
      com.google.android.material.R.attr.colorPrimary,
      Color.TRANSPARENT,
    )
  }.getOrNull()?.takeIf { Color.alpha(it) != 0 }

  private fun prefs(context: Context) =
    context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}
