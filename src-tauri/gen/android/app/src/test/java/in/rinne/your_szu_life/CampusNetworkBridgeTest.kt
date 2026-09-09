package `in`.rinne.your_szu_life

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class CampusNetworkBridgeTest {
  @Test
  fun normalizesUnknownSsidToNull() {
    assertEquals("SZU_WLAN", normalizeWifiSsid("  SZU_WLAN  "))
    assertNull(normalizeWifiSsid(null))
    assertNull(normalizeWifiSsid(""))
    assertNull(normalizeWifiSsid("<unknown ssid>"))
    assertNull(normalizeWifiSsid("unknown ssid"))
  }

  @Test
  fun serializesStableSnapshotShapeWhenOptionalValuesAreMissing() {
    assertEquals(
        "{\"connected\":true,\"ssid\":null,\"ip\":\"172.30.1.2\",\"transport\":null,\"capabilitiesAvailable\":null,\"wifiPermission\":null,\"locationEnabled\":null,\"hasPrivate172Ipv4\":false,\"wifiConnected\":false,\"wifiIp\":null,\"wifiHasPrivate172Ipv4\":false}",
        serializeNetworkSnapshotJson(true, null, "172.30.1.2"),
    )
    assertEquals(
        "{\"connected\":false,\"ssid\":\"SZU_WLAN\",\"ip\":null,\"transport\":null,\"capabilitiesAvailable\":null,\"wifiPermission\":null,\"locationEnabled\":null,\"hasPrivate172Ipv4\":false,\"wifiConnected\":false,\"wifiIp\":null,\"wifiHasPrivate172Ipv4\":false}",
        serializeNetworkSnapshotJson(false, "SZU_WLAN", null),
    )
  }

  @Test
  fun recognizesOnlyRfc1918Private172Range() {
    assertEquals(true, isPrivate172Ipv4("172.16.0.1"))
    assertEquals(true, isPrivate172Ipv4("172.30.1.2"))
    assertEquals(true, isPrivate172Ipv4("172.31.255.254"))
    assertEquals(false, isPrivate172Ipv4("172.15.255.254"))
    assertEquals(false, isPrivate172Ipv4("172.32.0.1"))
  }

  @Test
  fun serializesPrivate172Flag() {
    val json = serializeNetworkSnapshotJson(
        true,
        null,
        "10.0.0.2",
        hasPrivate172Ipv4 = false,
        wifiConnected = true,
        wifiIp = "172.30.1.2",
        wifiHasPrivate172Ipv4 = true,
    )
    assertEquals(true, json.contains("\"wifiConnected\":true"))
    assertEquals(true, json.contains("\"wifiIp\":\"172.30.1.2\""))
    assertEquals(true, json.contains("\"wifiHasPrivate172Ipv4\":true"))
  }
}
