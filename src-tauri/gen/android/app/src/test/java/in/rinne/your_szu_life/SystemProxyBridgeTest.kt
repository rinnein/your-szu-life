package `in`.rinne.your_szu_life

import java.net.InetSocketAddress
import java.net.Proxy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Pure-function tests; no emulator or Robolectric needed. */
class SystemProxyBridgeTest {
  @Test
  fun directSelectionMeansNoProxy() {
    assertEquals("""{"mode":"none","url":null}""", proxyJsonFromSelection(Proxy.Type.DIRECT, null))
  }

  @Test
  fun httpProxyBecomesAnHttpUrl() {
    assertEquals(
        """{"mode":"manual","url":"http://10.0.0.2:8080"}""",
        proxyJsonFromSelection(Proxy.Type.HTTP, InetSocketAddress("10.0.0.2", 8080)),
    )
  }

  @Test
  fun socksProxyUsesRemoteDns() {
    assertEquals(
        """{"mode":"manual","url":"socks5h://10.0.0.2:1080"}""",
        proxyJsonFromSelection(Proxy.Type.SOCKS, InetSocketAddress("10.0.0.2", 1080)),
    )
  }

  @Test
  fun ipv6HostsAreBracketed() {
    assertEquals("[fd00::1]", bracketHost("fd00::1"))
    assertEquals("[fd00::1]", bracketHost("[fd00::1]"))
    assertEquals("proxy.example", bracketHost("proxy.example"))
  }

  @Test
  fun unusableSelectionsReportUnavailable() {
    assertEquals(
        """{"mode":"unavailable","url":null}""",
        proxyJsonFromSelection(Proxy.Type.HTTP, null),
    )
    assertEquals(
        """{"mode":"unavailable","url":null}""",
        proxyJsonFromSelection(Proxy.Type.HTTP, InetSocketAddress("10.0.0.2", 0)),
    )
  }

  @Test
  fun jsonKeepsNullExplicit() {
    assertTrue(proxyJson("none", null).contains("\"url\":null"))
    assertTrue(proxyJson("manual", "http://h:1").contains("\"url\":\"http://h:1\""))
  }
}
