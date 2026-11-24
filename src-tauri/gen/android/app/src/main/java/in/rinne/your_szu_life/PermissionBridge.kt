package `in`.rinne.your_szu_life

import android.Manifest
import android.app.Activity
import android.app.AlarmManager
import android.app.NotificationManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.location.LocationManager
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import java.lang.ref.WeakReference
import java.util.Locale

private data class PermissionState(
    val id: String,
    val status: String,
    val canRequest: Boolean,
    val note: String? = null,
)

/**
 * `Manifest.permission.POST_PROMOTED_NOTIFICATIONS` 要到 API 37 才出现在 SDK
 * 常量里，而 compileSdk 是 36；这里直接写字面量，API 36 上只查系统开关、
 * 不发起运行时申请。
 */
private const val POST_PROMOTED_NOTIFICATIONS = "android.permission.POST_PROMOTED_NOTIFICATIONS"

/** API 36 起有 promoted 通知能力，API 37 起才需要申请对应的运行时权限。 */
private const val PROMOTED_NOTIFICATIONS_PERMISSION_SDK = 37

private fun jsonQuote(value: String): String = buildString(value.length + 2) {
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

private fun PermissionState.toJson(): String = buildString {
  append("{\"id\":")
  append(jsonQuote(id))
  append(",\"status\":")
  append(jsonQuote(status))
  append(",\"canRequest\":")
  append(canRequest)
  note?.let {
    append(",\"note\":")
    append(jsonQuote(it))
  }
  append('}')
}

/**
 * Android 系统权限桥。
 *
 * Rust 只按类名反射调用静态方法，因此这里的方法必须保持 `@JvmStatic`；
 * 权限申请是异步的，结果统一经 [nativePermissionResult] 回传，命令侧用
 * oneshot 等待，和 [StorageDirectoryBridge] 的目录选择器是同一套范式。
 *
 * 权限项 id 与 Rust `permissions::PermissionId`、前端 `src/permissions.ts`
 * 三处共享，新增一项必须同步修改。
 */
object PermissionBridge {
  private val granted = PackageManager.PERMISSION_GRANTED

  @Volatile private var applicationContext: Context? = null
  @Volatile private var activity: WeakReference<Activity> = WeakReference(null)

  @JvmStatic external fun nativePermissionResult(status: String, payload: String)

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

  /** 全量状态快照；上下文不可用时返回空数组，Rust 会按 unknown 补齐。 */
  @JvmStatic
  fun statusJson(): String {
    val context = applicationContext ?: return "[]"
    return listOf(
            notificationsState(context),
            promotedState(context),
            locationState(context),
            nearbyWifiState(context),
            cameraState(context),
            installPackagesState(context),
            exactAlarmState(context),
            autoStartState(),
            batteryOptimizationState(context),
        )
        .joinToString(prefix = "[", postfix = "]", separator = ",", transform = PermissionState::toJson)
  }

  /**
   * 申请给定 id 对应的运行时权限；`installPackages` / `exactAlarm` 只能跳系统
   * 设置，这里直接忽略。没有缺失项时立即回调，批量授权后不会再空弹一次。
   */
  @JvmStatic
  fun requestPermissions(keysCsv: String) {
    runCatching {
          val context = applicationContext ?: return fail("应用上下文不可用")
          val current = activity.get() ?: return fail("当前没有可用的界面")
          if (current.isFinishing) return fail("界面正在关闭")
          val missing = LinkedHashSet<String>()
          for (key in keysCsv.split(',').map(String::trim).filter(String::isNotEmpty)) {
            when (key) {
              "notifications" -> {
                if (canRequestNotifications(context)) {
                  missing += Manifest.permission.POST_NOTIFICATIONS
                }
              }
              "promotedNotifications" -> {
                if (Build.VERSION.SDK_INT >= PROMOTED_NOTIFICATIONS_PERMISSION_SDK &&
                    context.checkSelfPermission(POST_PROMOTED_NOTIFICATIONS) != granted) {
                  missing += POST_PROMOTED_NOTIFICATIONS
                }
              }
              "location" -> {
                if (context.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) != granted) {
                  missing += Manifest.permission.ACCESS_FINE_LOCATION
                }
                if (context.checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) != granted) {
                  missing += Manifest.permission.ACCESS_COARSE_LOCATION
                }
              }
              "nearbyWifi" -> {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                    context.checkSelfPermission(Manifest.permission.NEARBY_WIFI_DEVICES) != granted) {
                  missing += Manifest.permission.NEARBY_WIFI_DEVICES
                }
              }
              "camera" -> {
                if (context.checkSelfPermission(Manifest.permission.CAMERA) != granted) {
                  missing += Manifest.permission.CAMERA
                }
              }
              "installPackages", "exactAlarm", "autoStart", "batteryOptimization" -> Unit
              else -> return fail("未知的权限项：$key")
            }
          }
          if (missing.isEmpty()) {
            nativePermissionResult("ok", statusJson())
            return
          }
          current.runOnUiThread {
            runCatching {
                  val host = current as? WryActivity
                      ?: return@runCatching fail("当前界面不支持权限申请")
                  if (host.isFinishing) return@runCatching fail("界面正在关闭")
                  host.requestPermissions(missing.toTypedArray()) {
                    nativePermissionResult("ok", statusJson())
                  }
                }
                .onFailure { fail("无法申请权限：${it.message ?: "未知错误"}") }
          }
        }
        .onFailure { fail("无法申请权限：${it.message ?: "未知错误"}") }
  }

  /** 打开与某一项权限相关的系统设置；空串表示成功。 */
  @JvmStatic
  fun openSettings(target: String): String {
    val current = activity.get() ?: return "当前没有可用的界面"
    if (current.isFinishing) return "界面正在关闭"
    val packageUri = Uri.fromParts("package", current.packageName, null)
    val appDetails = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, packageUri)
    // 自启动没有官方入口：按品牌尝试私有页面，全部不可用时退回应用详情。
    if (target == "autoStart") {
      if (startVendorAutoStart(current)) return ""
      return runCatching {
            current.startActivity(appDetails)
            ""
          }
          .getOrElse { "无法打开系统设置：${it.message ?: "未知错误"}" }
    }
    val intent = when (target) {
      "app" -> appDetails
      "notifications" ->
          if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                .putExtra(Settings.EXTRA_APP_PACKAGE, current.packageName)
          } else {
            appDetails
          }
      "location" -> Intent(Settings.ACTION_LOCATION_SOURCE_SETTINGS)
      "installPackages" ->
          if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, packageUri)
          } else {
            appDetails
          }
      "exactAlarm" ->
          if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            Intent(Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM, packageUri)
          } else {
            appDetails
          }
      "batteryOptimization" -> batteryOptimizationIntent(current, packageUri, appDetails)
      else -> return "未知的设置入口：$target"
    }
    if (intent.resolveActivity(current.packageManager) == null) {
      return runCatching {
            current.startActivity(appDetails)
            ""
          }
          .getOrElse { "无法打开系统设置：${it.message ?: "未知错误"}" }
    }
    return runCatching {
          current.startActivity(intent)
          ""
        }
        .getOrElse { "无法打开系统设置：${it.message ?: "未知错误"}" }
  }

  private fun fail(message: String) {
    nativePermissionResult("error", message)
  }

  private fun check(context: Context, permission: String): Boolean =
      context.checkSelfPermission(permission) == granted

  private fun notificationsState(context: Context): PermissionState {
    val enabled = try {
      (context.getSystemService(Context.NOTIFICATION_SERVICE) as? NotificationManager)
          ?.areNotificationsEnabled() == true
    } catch (_: Throwable) {
      false
    }
    return PermissionState(
        id = "notifications",
        status = if (enabled) "granted" else "denied",
        canRequest = canRequestNotifications(context),
        note = if (enabled) null else "可在系统设置中允许本应用发送通知",
    )
  }

  private fun canRequestNotifications(context: Context): Boolean =
      Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
          !check(context, Manifest.permission.POST_NOTIFICATIONS)

  private fun promotedState(context: Context): PermissionState {
    if (Build.VERSION.SDK_INT < 36) {
      return PermissionState(
          id = "promotedNotifications",
          status = "unsupported",
          canRequest = false,
          note = "需要 Android 16 及以上的实时更新支持",
      )
    }
    val posted = try {
      (context.getSystemService(Context.NOTIFICATION_SERVICE) as? NotificationManager)
          ?.canPostPromotedNotifications() == true
    } catch (_: Throwable) {
      false
    }
    val requestable =
        Build.VERSION.SDK_INT >= PROMOTED_NOTIFICATIONS_PERMISSION_SDK &&
            !check(context, POST_PROMOTED_NOTIFICATIONS)
    return PermissionState(
        id = "promotedNotifications",
        status = if (posted) "granted" else "denied",
        canRequest = requestable,
        note = if (!posted) {
          if (requestable) "系统尚未允许实时更新" else "可在系统设置中开启实时更新"
        } else {
          null
        },
    )
  }

  private fun locationState(context: Context): PermissionState {
    val precise = check(context, Manifest.permission.ACCESS_FINE_LOCATION)
    val coarse = check(context, Manifest.permission.ACCESS_COARSE_LOCATION)
    if (!precise && !coarse) {
      return PermissionState(
          id = "location",
          status = "denied",
          canRequest = true,
          note = "校园网 Wi-Fi 识别与签到「使用当前位置」需要定位权限",
      )
    }
    val enabled = try {
      val manager = context.getSystemService(Context.LOCATION_SERVICE) as? LocationManager
      manager == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.P || manager.isLocationEnabled
    } catch (_: Throwable) {
      true
    }
    return PermissionState(
        id = "location",
        status = if (enabled) "granted" else "systemDisabled",
        canRequest = false,
        note = if (enabled) null else "系统定位服务已关闭",
    )
  }

  private fun nearbyWifiState(context: Context): PermissionState {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
      return PermissionState("nearbyWifi", "granted", canRequest = false)
    }
    val grantedNow = check(context, Manifest.permission.NEARBY_WIFI_DEVICES)
    return PermissionState(
        id = "nearbyWifi",
        status = if (grantedNow) "granted" else "denied",
        canRequest = !grantedNow,
        note = if (grantedNow) null else "Android 13 起读取 Wi-Fi 名称需要「附近的设备」权限",
    )
  }

  private fun cameraState(context: Context): PermissionState {
    val grantedNow = check(context, Manifest.permission.CAMERA)
    return PermissionState(
        id = "camera",
        status = if (grantedNow) "granted" else "denied",
        canRequest = !grantedNow,
        note = if (grantedNow) null else "扫码签到、加入共享房间与网页拍照需要相机权限",
    )
  }

  private fun installPackagesState(context: Context): PermissionState {
    val grantedNow =
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
          true
        } else {
          try {
            context.packageManager.canRequestPackageInstalls()
          } catch (_: Throwable) {
            false
          }
        }
    return PermissionState(
        id = "installPackages",
        status = if (grantedNow) "granted" else "denied",
        canRequest = false,
        note = if (grantedNow) null else "应用内更新安装需要允许安装未知应用",
    )
  }

  private fun exactAlarmState(context: Context): PermissionState {
    val grantedNow =
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
          true
        } else {
          try {
            (context.getSystemService(Context.ALARM_SERVICE) as? AlarmManager)
                ?.canScheduleExactAlarms() == true
          } catch (_: Throwable) {
            false
          }
        }
    return PermissionState(
        id = "exactAlarm",
        status = if (grantedNow) "granted" else "denied",
        canRequest = false,
        note = if (grantedNow) null else "未开启时系统可能延迟几分钟触发提醒",
    )
  }

  /**
   * 厂商「自启动 / 后台运行」没有官方接口，也无法读取是否已开启，因此状态恒为
   * `manual`，由用户到厂商设置里确认。
   */
  private fun autoStartState(): PermissionState =
      PermissionState(
          id = "autoStart",
          status = "manual",
          canRequest = false,
          note = "各厂商的「自启动 / 后台运行」入口不同，应用无法读取是否已开启；关闭时重启或清理后台后可能收不到提醒",
      )

  private fun batteryOptimizationState(context: Context): PermissionState {
    val ignoring =
        try {
          (context.getSystemService(Context.POWER_SERVICE) as? PowerManager)
              ?.isIgnoringBatteryOptimizations(context.packageName) == true
        } catch (_: Throwable) {
          false
        }
    return PermissionState(
        id = "batteryOptimization",
        status = if (ignoring) "granted" else "denied",
        canRequest = false,
        note = if (ignoring) null else "未加入电池优化白名单时，后台提醒可能被延迟或拦截",
    )
  }

  /** 按品牌尝试已知的私有自启动页面；全部不可用时返回 `false` 交给应用详情兜底。 */
  private fun startVendorAutoStart(activity: Activity): Boolean {
    val brand = "${Build.MANUFACTURER} ${Build.BRAND}".lowercase(Locale.ROOT)
    val components = buildList {
      if (brand.contains("xiaomi") ||
          brand.contains("redmi") ||
          brand.contains("poco") ||
          brand.contains("blackshark")) {
        add(
            "com.miui.securitycenter" to
                "com.miui.permcenter.autostart.AutoStartManagementActivity")
      }
      if (brand.contains("oppo") ||
          brand.contains("realme") ||
          brand.contains("oneplus") ||
          brand.contains("oplus")) {
        add(
            "com.coloros.safecenter" to
                "com.coloros.safecenter.permission.startup.StartupAppListActivity")
        add(
            "com.coloros.safecenter" to
                "com.coloros.safecenter.startupapp.StartupAppListActivity")
        add(
            "com.oplus.safecenter" to
                "com.oplus.safecenter.permission.startup.StartupAppListActivity")
      }
      if (brand.contains("vivo") || brand.contains("iqoo")) {
        add(
            "com.vivo.permissionmanager" to
                "com.vivo.permissionmanager.activity.BgStartUpManagerActivity")
        add("com.iqoo.secure" to "com.iqoo.secure.ui.phoneoptimize.AddWhiteListActivity")
      }
      if (brand.contains("huawei") || brand.contains("honor")) {
        add(
            "com.huawei.systemmanager" to
                "com.huawei.systemmanager.startupmgr.ui.StartupNormalAppListActivity")
        add(
            "com.huawei.systemmanager" to
                "com.huawei.systemmanager.optimize.process.ProtectActivity")
      }
    }
    for ((packageName, className) in components) {
      try {
        activity.startActivity(Intent().setComponent(ComponentName(packageName, className)))
        return true
      } catch (_: Throwable) {
        // 该品牌/版本可能没有这个页面，继续尝试下一个。
      }
    }
    return false
  }

  /** 优先直达本应用的电池优化申请页；不可用时退回白名单列表页。 */
  private fun batteryOptimizationIntent(
      context: Context,
      packageUri: Uri,
      fallback: Intent,
  ): Intent {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) return fallback
    val direct = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, packageUri)
    if (direct.resolveActivity(context.packageManager) != null) return direct
    val list = Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)
    return if (list.resolveActivity(context.packageManager) != null) list else fallback
  }
}
