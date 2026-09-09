package `in`.rinne.your_szu_life

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import android.net.ConnectivityManager
import android.net.wifi.WifiManager
import android.net.wifi.SupplicantState
import android.os.Build
import androidx.core.app.ActivityCompat
import java.lang.ref.WeakReference
import java.net.Inet4Address

internal fun normalizeWifiSsid(value: String?): String? {
  val normalized = value?.trim() ?: return null
  if (normalized.isEmpty()
      || normalized.equals("<unknown ssid>", ignoreCase = true)
      || normalized.equals("unknown ssid", ignoreCase = true)) {
    return null
  }
  return normalized
}

internal fun isPrivate172Ipv4(value: String): Boolean {
  val octets = value.split('.').mapNotNull { it.toIntOrNull() }
  return octets.size == 4 && octets[0] == 172 && octets[1] in 16..31
}

internal fun serializeNetworkSnapshotJson(
    connected: Boolean,
    ssid: String?,
    ip: String?,
    transport: String? = null,
    capabilitiesAvailable: Boolean? = null,
    wifiPermission: String? = null,
    locationEnabled: Boolean? = null,
    hasPrivate172Ipv4: Boolean = false,
    wifiConnected: Boolean = false,
    wifiIp: String? = null,
    wifiHasPrivate172Ipv4: Boolean = false,
): String {
  fun quote(value: String): String = buildString(value.length + 2) {
    append('"')
    value.forEach { character ->
      when (character) {
        '\\' -> append("\\\\")
        '"' -> append("\\\"")
        '\b' -> append("\\b")
        '\u000C' -> append("\\f")
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
  fun appendNullableString(builder: StringBuilder, value: String?) {
    builder.append(value?.let(::quote) ?: "null")
  }
  fun appendNullableBoolean(builder: StringBuilder, value: Boolean?) {
    builder.append(value?.toString() ?: "null")
  }
  return buildString {
    append("{\"connected\":")
    append(connected)
    append(",\"ssid\":")
    appendNullableString(this, ssid)
    append(",\"ip\":")
    appendNullableString(this, ip)
    append(",\"transport\":")
    appendNullableString(this, transport)
    append(",\"capabilitiesAvailable\":")
    appendNullableBoolean(this, capabilitiesAvailable)
    append(",\"wifiPermission\":")
    appendNullableString(this, wifiPermission)
    append(",\"locationEnabled\":")
    appendNullableBoolean(this, locationEnabled)
    append(",\"hasPrivate172Ipv4\":")
    append(hasPrivate172Ipv4)
    append(",\"wifiConnected\":")
    append(wifiConnected)
    append(",\"wifiIp\":")
    appendNullableString(this, wifiIp)
    append(",\"wifiHasPrivate172Ipv4\":")
    append(wifiHasPrivate172Ipv4)
    append('}')
  }
}

/** Android network and Wi-Fi permission bridge used by the Rust campus monitor. */
object CampusNetworkBridge {
  private const val WIFI_PERMISSION_REQUEST_CODE = 0x535a

  @Volatile private var applicationContext: Context? = null
  @Volatile private var activity: WeakReference<Activity> = WeakReference(null)

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
  fun getNetworkSnapshotJson(): String {
    return runCatching { readNetworkSnapshotJson() }.getOrElse {
      serializeNetworkSnapshotJson(false, null, null, capabilitiesAvailable = false)
    }
  }

  private fun readNetworkSnapshotJson(): String {
    val context = applicationContext
        ?: return serializeNetworkSnapshotJson(false, null, null, capabilitiesAvailable = false)
    val connectivity =
        context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
            ?: return serializeNetworkSnapshotJson(false, null, null, capabilitiesAvailable = false)

    val defaultNetwork = runCatching { connectivity.activeNetwork }.getOrNull()
    val defaultCapabilities = defaultNetwork?.let {
      runCatching { connectivity.getNetworkCapabilities(it) }.getOrNull()
    }
    val defaultLinkProperties = defaultNetwork?.let {
      runCatching { connectivity.getLinkProperties(it) }.getOrNull()
    }
    val defaultIpv4 = defaultLinkProperties?.let(::ipv4Addresses).orEmpty()

    val wifiInfo = runCatching {
      (context.getSystemService(Context.WIFI_SERVICE) as? WifiManager)?.connectionInfo
    }.getOrNull()
    val wifiAssociated = wifiInfo?.supplicantState == SupplicantState.COMPLETED
    val networks = runCatching { connectivity.allNetworks.toList() }.getOrDefault(emptyList())
    val wifiCandidates = networks.mapNotNull { network ->
      val capabilities = runCatching { connectivity.getNetworkCapabilities(network) }.getOrNull()
      val linkProperties = runCatching { connectivity.getLinkProperties(network) }.getOrNull()
      val isWifi = capabilities?.hasTransport(android.net.NetworkCapabilities.TRANSPORT_WIFI) == true ||
          (wifiAssociated && linkProperties?.interfaceName?.startsWith("wlan") == true)
      if (isWifi) linkProperties?.let(::ipv4Addresses).orEmpty() else null
    }
    val wifiIpv4Addresses = wifiCandidates.flatten().distinct()
    val wifiConnected = wifiCandidates.isNotEmpty() || wifiAssociated
    val wifiIp = wifiIpv4Addresses.firstOrNull()
    val wifiHasPrivate172Ipv4 = wifiIpv4Addresses.any(::isPrivate172Ipv4)

    // A captive portal may keep Wi-Fi associated while Android chooses cellular as the
    // default validated network. Presence of either network is the connectivity truth.
    val connected = defaultNetwork != null || wifiConnected
    val ipv4Addresses = (defaultIpv4 + wifiIpv4Addresses).distinct()
    val ip = ipv4Addresses.firstOrNull()
    val hasPrivate172Ipv4 = ipv4Addresses.any(::isPrivate172Ipv4)
    val transport = when {
      defaultCapabilities?.hasTransport(android.net.NetworkCapabilities.TRANSPORT_WIFI) == true -> "wifi"
      defaultCapabilities?.hasTransport(android.net.NetworkCapabilities.TRANSPORT_CELLULAR) == true -> "cellular"
      defaultCapabilities?.hasTransport(android.net.NetworkCapabilities.TRANSPORT_ETHERNET) == true -> "ethernet"
      defaultCapabilities != null -> "other"
      else -> null
    }
    val permission = wifiPermissionState(context)
    val locationEnabled = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
      (context.getSystemService(Context.LOCATION_SERVICE) as? LocationManager)?.isLocationEnabled
    } else {
      null
    }
    val ssid = readSsid(context)
    return serializeNetworkSnapshotJson(
        connected,
        ssid,
        ip,
        transport,
        defaultCapabilities != null,
        permission,
        locationEnabled,
        hasPrivate172Ipv4,
        wifiConnected,
        wifiIp,
        wifiHasPrivate172Ipv4,
    )
  }

  private fun ipv4Addresses(linkProperties: android.net.LinkProperties): List<String> =
    linkProperties.linkAddresses.mapNotNull { link ->
      val address = link.address
      if (address is Inet4Address) address.hostAddress else null
    }

  @JvmStatic
  fun requestWifiPermissions() {
    runCatching {
      val currentActivity = activity.get() ?: return
      if (currentActivity.isFinishing ||
          (Build.VERSION.SDK_INT >= Build.VERSION_CODES.JELLY_BEAN_MR1 && currentActivity.isDestroyed)) {
        return
      }
      val missing = mutableListOf<String>()
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        if (currentActivity.checkSelfPermission(Manifest.permission.NEARBY_WIFI_DEVICES) !=
            PackageManager.PERMISSION_GRANTED) {
          missing += Manifest.permission.NEARBY_WIFI_DEVICES
        }
      } else if (currentActivity.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) !=
          PackageManager.PERMISSION_GRANTED) {
        missing += Manifest.permission.ACCESS_FINE_LOCATION
      }
      if (missing.isEmpty()) return
      currentActivity.runOnUiThread {
        runCatching {
          if (!currentActivity.isFinishing &&
              (Build.VERSION.SDK_INT < Build.VERSION_CODES.JELLY_BEAN_MR1 ||
                  !currentActivity.isDestroyed)) {
            ActivityCompat.requestPermissions(
                currentActivity,
                missing.toTypedArray(),
                WIFI_PERMISSION_REQUEST_CODE,
            )
          }
        }
      }
    }
  }

  private fun readSsid(context: Context): String? {
    if (!canReadSsid(context)) return null
    val wifi = context.getSystemService(Context.WIFI_SERVICE) as? WifiManager ?: return null
    return runCatching { normalizeWifiSsid(wifi.connectionInfo?.ssid) }.getOrNull()
  }

  private fun wifiPermissionState(context: Context): String {
    val permission = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      Manifest.permission.NEARBY_WIFI_DEVICES
    } else {
      Manifest.permission.ACCESS_FINE_LOCATION
    }
    return if (context.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED) {
      "granted"
    } else {
      "denied"
    }
  }

  private fun canReadSsid(context: Context): Boolean {
    val permission = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      Manifest.permission.NEARBY_WIFI_DEVICES
    } else {
      Manifest.permission.ACCESS_FINE_LOCATION
    }
    if (context.checkSelfPermission(permission) != PackageManager.PERMISSION_GRANTED) {
      return false
    }
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) return true
    val location = context.getSystemService(Context.LOCATION_SERVICE) as? LocationManager
        ?: return false
    return location.isLocationEnabled
  }

}
