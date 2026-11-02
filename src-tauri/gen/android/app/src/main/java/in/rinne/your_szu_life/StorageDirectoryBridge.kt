package `in`.rinne.your_szu_life

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.storage.StorageManager
import android.provider.DocumentsContract
import java.io.File
import java.lang.ref.WeakReference

private const val EXTERNAL_STORAGE_AUTHORITY = "com.android.externalstorage.documents"

/**
 * 解析 SAF 文档树的展示路径。
 *
 * `primary:` 映射到内部共享存储根；其它卷用卷 UUID 在 [volumeRoots] 里查。
 * 查不到时退回树内相对路径，宁可少一节前缀也不编造绝对路径。
 */
internal fun resolveTreeDisplayPath(
    documentId: String?,
    primaryRoot: String,
    volumeRoots: Map<String, String>,
    fallback: String,
): String {
  if (documentId == null) return fallback
  val separator = documentId.indexOf(':')
  if (separator < 0) return documentId
  val volume = documentId.substring(0, separator)
  val relative = documentId.substring(separator + 1).trim('/')
  val root = if (volume == "primary") primaryRoot else volumeRoots[volume] ?: return relative
  return if (relative.isEmpty()) root else "$root/$relative"
}

internal fun serializeDirectoryJson(displayPath: String, treeUri: String, hasEntries: Boolean): String {
  fun quote(value: String): String = buildString(value.length + 2) {
    append('"')
    value.forEach { character ->
      when (character) {
        '\\' -> append("\\\\")
        '"' -> append("\\\"")
        '\n' -> append("\\n")
        '\r' -> append("\\r")
        '\t' -> append("\\t")
        else -> {
          if (character.code < 0x20) {
            append("\\u")
            append(character.code.toString(16).padStart(4, '0'))
          } else {
            append(character)
          }
        }
      }
    }
    append('"')
  }
  return buildString {
    append("{\"displayPath\":")
    append(quote(displayPath))
    append(",\"treeUri\":")
    append(quote(treeUri))
    append(",\"hasEntries\":")
    append(hasEntries)
    append('}')
  }
}

/**
 * Android 存储目录桥。
 *
 * SAF 授权只对 `ContentResolver` 生效，所以这里承担三件事：打开系统的文件夹
 * 选择器、在选中的文档树里建目录/写探针，以及在 aria2 下载完成后把暂存文件
 * 复制进文档树。
 */
object StorageDirectoryBridge {
  private const val REQUEST_PICK_DIRECTORY = 0x535b
  private const val PROBE_FILE_NAME = ".泥深-write-test"
  private const val BUFFER_SIZE = 64 * 1024

  @Volatile private var applicationContext: Context? = null
  @Volatile private var activity: WeakReference<Activity> = WeakReference(null)

  @JvmStatic external fun nativeDirectoryPicked(status: String, payload: String)

  @JvmStatic
  fun bindActivity(activity: Activity) {
    this.applicationContext = activity.applicationContext
    this.activity = WeakReference(activity)
  }

  @JvmStatic
  fun unbindActivity(activity: Activity) {
    if (this.activity.get() === activity) {
      this.activity = WeakReference(null)
    }
  }

  @JvmStatic
  fun pickDirectory(initialPath: String) {
    val current = activity.get()
    if (current == null) {
      nativeDirectoryPicked("error", "当前没有可用的界面")
      return
    }
    current.runOnUiThread {
      runCatching {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
          addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
          addFlags(Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
          addFlags(Intent.FLAG_GRANT_PREFIX_URI_PERMISSION)
          if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            initialDocumentUri(initialPath)?.let {
              putExtra(DocumentsContract.EXTRA_INITIAL_URI, it)
            }
          }
        }
        current.startActivityForResult(intent, REQUEST_PICK_DIRECTORY)
      }.onFailure {
        nativeDirectoryPicked("error", "无法打开文件夹选择器：${it.message ?: "未知错误"}")
      }
    }
  }

  /** 返回 true 表示该结果由本桥处理。 */
  @JvmStatic
  fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?): Boolean {
    if (requestCode != REQUEST_PICK_DIRECTORY) return false
    val context = applicationContext
    val uri = data?.data
    if (resultCode != Activity.RESULT_OK || context == null || uri == null) {
      nativeDirectoryPicked("cancelled", "")
      return true
    }
    runCatching {
      context.contentResolver.takePersistableUriPermission(
          uri,
          Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
      )
    }
    val hasEntries = runCatching { hasChildren(context, uri) }.getOrElse {
      nativeDirectoryPicked("error", "无法读取所选目录：${it.message ?: "未知错误"}")
      return true
    }
    val displayPath = runCatching { displayPath(context, uri) }.getOrElse { uri.toString() }
    nativeDirectoryPicked("ok", serializeDirectoryJson(displayPath, uri.toString(), hasEntries))
    return true
  }

  @JvmStatic
  fun inspectTree(uriString: String): String {
    val context = applicationContext ?: return ""
    val uri = runCatching { Uri.parse(uriString) }.getOrNull() ?: return ""
    val hasEntries = runCatching { hasChildren(context, uri) }.getOrElse { return "" }
    val display = runCatching { displayPath(context, uri) }.getOrElse { uriString }
    return serializeDirectoryJson(display, uriString, hasEntries)
  }

  @JvmStatic
  fun checkAccess(uriString: String): Boolean {
    val context = applicationContext ?: return false
    val uri = runCatching { Uri.parse(uriString) }.getOrNull() ?: return false
    return runCatching {
      context.contentResolver.persistedUriPermissions.any { it.uri == uri && it.isWritePermission }
    }.getOrDefault(false)
  }

  /** 应用的标准缓存目录；优先外部缓存，容量足够放录像。 */
  @JvmStatic
  fun stagingRoot(): String {
    val context = applicationContext ?: return ""
    val directory = context.externalCacheDir ?: context.cacheDir
    return directory?.absolutePath ?: ""
  }

  /**
   * 只取回断点续传真正需要的最小文件集合。
   *
   * 仅当目标目录里存在 `<fileName>.aria2` 时才动：先把控制文件取回，再取回可能
   * 只下载了一部分的 `<fileName>`。没有控制文件说明目标文件不是中断的下载，
   * 一个字节都不复制。
   */
  @JvmStatic
  fun exportResumeState(
      uriString: String,
      relativeDir: String,
      fileName: String,
      destinationDir: String,
  ): String {
    val context = applicationContext ?: return "应用上下文不可用"
    val treeUri = runCatching { Uri.parse(uriString) }.getOrNull() ?: return "目录授权无效"
    return runCatching {
      val documentId = findDocumentId(context, treeUri, relativeDir) ?: return ""
      val childrenUri = childDocumentsUri(treeUri, documentId)
      val controlName = "$fileName.aria2"
      val controlId = childDocumentIdByName(context, childrenUri, controlName) ?: return ""
      val destination = File(destinationDir)
      if (!destination.isDirectory && !destination.mkdirs()) {
        return "无法创建暂存目录"
      }
      copyDocument(context, treeUri, controlId, File(destination, controlName))
      childDocumentIdByName(context, childrenUri, fileName)?.let { fileId ->
        copyDocument(context, treeUri, fileId, File(destination, fileName))
      }
      ""
    }.getOrElse { "读取续传文件失败：${it.message ?: "未知错误"}" }
  }

  @JvmStatic
  fun ensureDirectory(uriString: String, relativeBase: String): String {
    val context = applicationContext ?: return "应用上下文不可用"
    val treeUri = runCatching { Uri.parse(uriString) }.getOrNull() ?: return "目录授权无效"
    return runCatching {
      val documentId = ensureDocumentId(context, treeUri, relativeBase)
          ?: return "无法创建目标目录"
      val probe = DocumentsContract.createDocument(
          context.contentResolver,
          childDocumentsUri(treeUri, documentId),
          "application/octet-stream",
          PROBE_FILE_NAME,
      ) ?: return "目录不可写"
      runCatching { DocumentsContract.deleteDocument(context.contentResolver, probe) }
      ""
    }.getOrElse { "目录不可写：${it.message ?: "未知错误"}" }
  }

  @JvmStatic
  fun publishFile(
      uriString: String,
      relativeDir: String,
      sourcePath: String,
      fileName: String,
  ): String {
    val context = applicationContext ?: return "应用上下文不可用"
    val treeUri = runCatching { Uri.parse(uriString) }.getOrNull() ?: return "目录授权无效"
    val source = File(sourcePath)
    if (!source.isFile) return "暂存文件不存在：$sourcePath"
    return runCatching {
      val documentId = ensureDocumentId(context, treeUri, relativeDir)
          ?: return "无法创建目标目录"
      val childrenUri = childDocumentsUri(treeUri, documentId)
      val existingId = childDocumentIdByName(context, childrenUri, fileName)
      val fileUri = existingId?.let { DocumentsContract.buildDocumentUriUsingTree(treeUri, it) }
          ?: DocumentsContract.createDocument(
              context.contentResolver,
              childrenUri,
              "application/octet-stream",
              fileName,
          )
          ?: return "无法创建文件：$fileName"
      try {
        val output = context.contentResolver.openOutputStream(fileUri, "wt")
            ?: return "无法打开目标文件：$fileName"
        output.use { out ->
          source.inputStream().use { input -> input.copyTo(out, BUFFER_SIZE) }
        }
        // 复制后核对目标大小，确认文件确实完整；服务方不报大小（-1）时跳过。
        val copiedSize = documentSize(context, fileUri)
        if (copiedSize >= 0 && copiedSize != source.length()) {
          if (existingId == null) {
            runCatching { DocumentsContract.deleteDocument(context.contentResolver, fileUri) }
          }
          return "保存不完整（预期 ${source.length()} 字节，实际 $copiedSize 字节）"
        }
        ""
      } catch (error: Exception) {
        // 新建的半截文件要清掉，避免用户看到一个损坏的录像。
        if (existingId == null) {
          runCatching { DocumentsContract.deleteDocument(context.contentResolver, fileUri) }
        }
        throw error
      }
    }.getOrElse { "写入失败：${it.message ?: "未知错误"}" }
  }

  @JvmStatic
  fun deleteFile(uriString: String, relativeDir: String, fileName: String): String {
    val context = applicationContext ?: return "应用上下文不可用"
    val treeUri = runCatching { Uri.parse(uriString) }.getOrNull() ?: return "目录授权无效"
    return runCatching {
      val documentId = findDocumentId(context, treeUri, relativeDir) ?: return ""
      val targetId = childDocumentIdByName(context, childDocumentsUri(treeUri, documentId), fileName)
          ?: return ""
      val target = DocumentsContract.buildDocumentUriUsingTree(treeUri, targetId)
      runCatching { DocumentsContract.deleteDocument(context.contentResolver, target) }
      ""
    }.getOrElse { "删除失败：${it.message ?: "未知错误"}" }
  }

  @JvmStatic
  fun releaseTree(uriString: String) {
    val context = applicationContext ?: return
    val uri = runCatching { Uri.parse(uriString) }.getOrNull() ?: return
    runCatching {
      context.contentResolver.releasePersistableUriPermission(
          uri,
          Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
      )
    }
  }

  private fun initialDocumentUri(initialPath: String): Uri? {
    val root = Environment.getExternalStorageDirectory().absolutePath
    val relative = initialPath.removePrefix(root).trim('/')
    if (relative.isEmpty()) return null
    return runCatching {
      DocumentsContract.buildDocumentUri(EXTERNAL_STORAGE_AUTHORITY, "primary:$relative")
    }.getOrNull()
  }

  private fun displayPath(context: Context, treeUri: Uri): String {
    val roots = mutableMapOf<String, String>()
    runCatching {
      val manager = context.getSystemService(Context.STORAGE_SERVICE) as? StorageManager
      manager?.storageVolumes?.forEach { volume ->
        val uuid = volume.uuid ?: return@forEach
        val directory = volume.directory?.absolutePath ?: return@forEach
        roots[uuid] = directory
      }
    }
    val primary = Environment.getExternalStorageDirectory().absolutePath
    val documentId =
        runCatching { DocumentsContract.getTreeDocumentId(treeUri) }.getOrNull()
    return resolveTreeDisplayPath(documentId, primary, roots, treeUri.toString())
  }

  private fun hasChildren(context: Context, treeUri: Uri): Boolean {
    val documentId = DocumentsContract.getTreeDocumentId(treeUri)
    return context.contentResolver.query(
        childDocumentsUri(treeUri, documentId),
        arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID),
        null,
        null,
        null,
    )?.use { cursor -> cursor.moveToFirst() } ?: false
  }

  private fun childDocumentsUri(treeUri: Uri, documentId: String): Uri =
      DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, documentId)

  private fun copyDocument(context: Context, treeUri: Uri, documentId: String, destination: File) {
    val documentUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId)
    val input = context.contentResolver.openInputStream(documentUri)
        ?: throw IllegalStateException("无法读取 ${destination.name}")
    input.use { source ->
      destination.outputStream().use { output ->
        source.copyTo(output, BUFFER_SIZE)
      }
    }
  }

  /** 目标文档的字节数；提供方不返回大小时为 -1。 */
  private fun documentSize(context: Context, documentUri: Uri): Long {
    return context.contentResolver.query(
        documentUri,
        arrayOf(DocumentsContract.Document.COLUMN_SIZE),
        null,
        null,
        null,
    )?.use { cursor ->
      if (cursor.moveToFirst() && !cursor.isNull(0)) cursor.getLong(0) else -1L
    } ?: -1L
  }

  private fun findDocumentId(context: Context, treeUri: Uri, relativeDir: String): String? {
    var currentId = DocumentsContract.getTreeDocumentId(treeUri)
    for (name in relativeDir.split('/').filter { it.isNotBlank() }) {
      currentId = childDocumentIdByName(context, childDocumentsUri(treeUri, currentId), name)
          ?: return null
    }
    return currentId
  }

  /** 逐级创建缺失目录，返回最终目录的文档 id。 */
  private fun ensureDocumentId(context: Context, treeUri: Uri, relativeDir: String): String? {
    var currentId = DocumentsContract.getTreeDocumentId(treeUri)
    for (name in relativeDir.split('/').filter { it.isNotBlank() }) {
      val existing = childDocumentIdByName(context, childDocumentsUri(treeUri, currentId), name)
      currentId = existing ?: run {
        val created = DocumentsContract.createDocument(
            context.contentResolver,
            childDocumentsUri(treeUri, currentId),
            DocumentsContract.Document.MIME_TYPE_DIR,
            name,
        ) ?: return null
        DocumentsContract.getDocumentId(created)
      }
    }
    return currentId
  }

  private fun childDocumentIdByName(context: Context, childrenUri: Uri, name: String): String? {
    context.contentResolver.query(
        childrenUri,
        arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
        ),
        null,
        null,
        null,
    )?.use { cursor ->
      while (cursor.moveToNext()) {
        if (cursor.getString(1) == name) return cursor.getString(0)
      }
    }
    return null
  }
}
