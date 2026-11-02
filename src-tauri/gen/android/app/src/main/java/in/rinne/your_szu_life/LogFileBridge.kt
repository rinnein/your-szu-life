package `in`.rinne.your_szu_life

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import java.io.File

/**
 * 把应用私有目录里的日志交给系统应用打开。
 *
 * tauri-plugin-opener 的 Android 实现会把路径当裸字符串转发（与 Kotlin 侧
 * `OpenArgs` 的对象契约不符），也不会为私有目录生成带读取授权的 URI。这里
 * 统一走 FileProvider：content:// + FLAG_GRANT_READ_URI_PERMISSION。
 */
object LogFileBridge {
  private const val MIME_TYPE = "text/plain"

  /** 返回空字符串表示成功，否则是可直接展示的中文错误信息。 */
  @JvmStatic
  fun openLogFile(context: Context, path: String): String {
    return try {
      val file = File(path)
      if (!file.isFile) return "日志文件不存在"

      val uri = FileProvider.getUriForFile(
        context,
        "${context.packageName}.fileprovider",
        file,
      )
      val view = Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(uri, MIME_TYPE)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        clipData = ClipData.newRawUri("log", uri)
      }
      val target = if (context.packageManager.queryIntentActivities(view, 0).isNotEmpty()) {
        Intent.createChooser(view, "打开日志")
      } else {
        val share = Intent(Intent.ACTION_SEND).apply {
          type = MIME_TYPE
          putExtra(Intent.EXTRA_STREAM, uri)
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
          clipData = ClipData.newRawUri("log", uri)
        }
        Intent.createChooser(share, "分享日志")
      }
      target.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      target.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      context.startActivity(target)
      ""
    } catch (error: ActivityNotFoundException) {
      "没有可用于打开日志的应用"
    } catch (error: Exception) {
      error.message ?: "无法打开日志文件"
    }
  }
}
