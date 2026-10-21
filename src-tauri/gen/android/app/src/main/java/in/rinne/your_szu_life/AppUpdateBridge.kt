package `in`.rinne.your_szu_life

import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import java.io.File
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors
import java.util.zip.GZIPInputStream

private class CountingInputStream(private val wrapped: InputStream) : InputStream() {
  var bytesRead: Long = 0L
    private set

  override fun read(): Int {
    val b = wrapped.read()
    if (b != -1) bytesRead++
    return b
  }

  override fun read(b: ByteArray, off: Int, len: Int): Int {
    val count = wrapped.read(b, off, len)
    if (count != -1) bytesRead += count
    return count
  }

  override fun close() = wrapped.close()
  override fun available() = wrapped.available()
}

object AppUpdateBridge {
  private val executor = Executors.newSingleThreadExecutor()
  @Volatile private var isDownloading = false

  @JvmStatic
  external fun nativeUpdateProgress(
      status: String,
      progress: Int,
      downloadedBytes: Long,
      totalBytes: Long,
      error: String?
  )

  @JvmStatic
  fun startUpdate(context: Context, urlsJson: String, isGzip: Boolean): Boolean {
    if (isDownloading) {
      return false
    }
    val urls: List<String> = try {
      val jsonArray = org.json.JSONArray(urlsJson)
      (0 until jsonArray.length()).map { jsonArray.getString(it) }
    } catch (_: Exception) {
      return false
    }
    if (urls.isEmpty()) {
      return false
    }

    isDownloading = true
    executor.execute {
      var success = false
      var lastError = "无法连接更新服务器"
      val targetFile = File(context.cacheDir, "update.apk")
      if (targetFile.exists()) {
        targetFile.delete()
      }

      for (urlString in urls) {
        var connection: HttpURLConnection? = null
        try {
          nativeUpdateProgress("downloading", 0, 0, 0, null)
          val url = URL(urlString)
          connection = (url.openConnection() as HttpURLConnection).apply {
            connectTimeout = 15000
            readTimeout = 60000
            instanceFollowRedirects = true
            setRequestProperty("User-Agent", "your-szu-life")
          }
          val code = connection.responseCode
          if (code !in 200..299) {
            lastError = "服务器返回 HTTP $code"
            connection.disconnect()
            continue
          }

          val totalBytes = connection.contentLengthLong
          val rawStream = CountingInputStream(connection.inputStream)
          val stream = if (isGzip) GZIPInputStream(rawStream) else rawStream

          targetFile.outputStream().use { output ->
            val buffer = ByteArray(64 * 1024)
            var lastProgressReport = 0L
            while (true) {
              val read = stream.read(buffer)
              if (read == -1) break
              output.write(buffer, 0, read)
              val downloaded = rawStream.bytesRead
              val now = System.currentTimeMillis()
              if (now - lastProgressReport > 150) {
                lastProgressReport = now
                val progress = if (totalBytes > 0) {
                  ((downloaded.toDouble() / totalBytes) * 100).toInt().coerceIn(0, 99)
                } else {
                  0
                }
                nativeUpdateProgress("downloading", progress, downloaded, totalBytes, null)
              }
            }
          }

          success = true
          break
        } catch (e: Exception) {
          lastError = e.message ?: "下载过程中发生错误"
          if (targetFile.exists()) {
            targetFile.delete()
          }
        } finally {
          connection?.disconnect()
        }
      }

      isDownloading = false
      if (success && targetFile.exists() && targetFile.length() > 0) {
        nativeUpdateProgress("installing", 100, targetFile.length(), targetFile.length(), null)
        installApk(context, targetFile)
      } else {
        nativeUpdateProgress("error", 0, 0, 0, lastError)
      }
    }
    return true
  }

  @JvmStatic
  fun installApk(context: Context, apkFile: File): Boolean {
    return try {
      val intent = Intent(Intent.ACTION_VIEW).apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        val uri = FileProvider.getUriForFile(
            context,
            "${context.packageName}.fileprovider",
            apkFile
        )
        setDataAndType(uri, "application/vnd.android.package-archive")
      }
      context.startActivity(intent)
      true
    } catch (e: Exception) {
      nativeUpdateProgress("error", 0, 0, 0, "无法打开安装程序: ${e.message}")
      false
    }
  }
}
