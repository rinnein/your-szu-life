package `in`.rinne.your_szu_life

import android.util.Log
import java.net.InetSocketAddress
import java.net.Proxy
import java.net.ProxySelector
import java.net.URI

/**
 * Reads the system proxy so the Rust side can route outgoing requests the same
 * way the WebView does.
 *
 * Android's `ProxySelector` already applies the system bypass list for the URI
 * we hand it, which is why the caller passes a probe URL of the protocol it
 * cares about. No Context is needed.
 */
object SystemProxyBridge {
  private const val TAG = "SystemProxyBridge"

  @JvmStatic
  fun getProxyJson(probeUrl: String): String {
    val selector = try {
      ProxySelector.getDefault()
    } catch (error: Throwable) {
      Log.w(TAG, "proxy selector unavailable", error)
      null
    } ?: return proxyJson("unavailable", null)

    val selected = try {
      selector.select(URI.create(probeUrl)).firstOrNull()
    } catch (error: Throwable) {
      Log.w(TAG, "proxy selection failed", error)
      null
    } ?: return proxyJson("none", null)

    return proxyJsonFromSelection(selected.type(), selected.address())
  }
}

/**
 * Pure helpers, unit-tested without an emulator (see SystemProxyBridgeTest).
 *
 * `address` is what `Proxy.address()` returned: an `InetSocketAddress` for HTTP
 * and SOCKS proxies, `null` for DIRECT.
 */
internal fun proxyJsonFromSelection(type: Proxy.Type, address: java.net.SocketAddress?): String {
  if (type == Proxy.Type.DIRECT) return proxyJson("none", null)
  val socket = address as? InetSocketAddress ?: return proxyJson("unavailable", null)
  val host = socket.hostString?.takeIf { it.isNotBlank() } ?: return proxyJson("unavailable", null)
  if (socket.port <= 0) return proxyJson("unavailable", null)
  val scheme = if (type == Proxy.Type.SOCKS) "socks5h" else "http"
  return proxyJson("manual", "$scheme://${bracketHost(host)}:${socket.port}")
}

/** IPv6 literals must be bracketed before they can be used as a URL authority. */
internal fun bracketHost(host: String): String =
    if (host.contains(':') && !host.startsWith("[")) "[$host]" else host

internal fun proxyJson(mode: String, url: String?): String {
  val urlField = if (url == null) "null" else "\"$url\""
  return """{"mode":"$mode","url":$urlField}"""
}
