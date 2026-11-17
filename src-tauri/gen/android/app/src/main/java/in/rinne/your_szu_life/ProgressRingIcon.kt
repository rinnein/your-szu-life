package `in`.rinne.your_szu_life

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import androidx.core.graphics.drawable.IconCompat

/**
 * 进度环小图标：系统对 small icon 只取 alpha 通道着色，因此底环画低不透明度、
 * 进度弧画全不透明，渲染出来就是「暗环 + 亮弧」。
 *
 * 实时活动每 15 秒重发一次通知 → 环每 15 秒前进一格（sleepy 的做法）。
 * 任何绘制失败都返回 null，调用方回退到静态时钟图标。
 */
object ProgressRingIcon {
  private const val SIZE = 96
  private const val STROKE = 12f
  private const val INSET = 12f
  private const val TRACK_ALPHA = 152
  private const val ARC_ALPHA = 255

  fun create(progress: Int): IconCompat? = try {
    val bitmap = Bitmap.createBitmap(SIZE, SIZE, Bitmap.Config.ARGB_8888)
    val canvas = Canvas(bitmap)
    val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
      style = Paint.Style.STROKE
      strokeWidth = STROKE
      strokeCap = Paint.Cap.ROUND
    }
    val oval = RectF(INSET, INSET, SIZE - INSET, SIZE - INSET)
    // 底环（暗）。
    paint.color = (TRACK_ALPHA shl 24) or 0xFFFFFF
    canvas.drawArc(oval, 0f, 360f, false, paint)
    // 进度弧（亮），从 12 点顺时针。
    val sweep = 360f * progress.coerceIn(0, 100) / 100f
    if (sweep > 1f) {
      paint.color = (ARC_ALPHA shl 24) or 0xFFFFFF
      canvas.drawArc(oval, -90f, sweep, false, paint)
    }
    IconCompat.createWithBitmap(bitmap)
  } catch (_: Throwable) {
    null
  }
}
