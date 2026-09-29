package `in`.rinne.your_szu_life

import java.net.URI

internal fun applicationOrigin(url: String?): String? = runCatching {
  if (url.isNullOrBlank()) return null
  val uri = URI(url)
  val scheme = uri.scheme?.lowercase()
  val host = uri.host?.lowercase()
  if (scheme !in setOf("http", "https") || host == null || uri.rawUserInfo != null) return null
  val port = uri.port
  val suffix = if (port == -1 || (scheme == "http" && port == 80) ||
    (scheme == "https" && port == 443)) "" else ":$port"
  "$scheme://$host$suffix"
}.getOrNull()

/** Pure state policy, separate from WebView and Activity lifecycle wiring. */
internal class ApplicationBackPolicy(
  private val origins: Set<String>,
) {
  var localDocument = false
    private set
  var enabled = false
    private set

  var action: String? = null
    private set
  var hasLogicalParent = false
    private set

  fun document(url: String?) {
    localDocument = applicationOrigin(url) in origins
    // Child windows yield even while loading or on an official remote page.
    // Native history inside a child must not mask the Activity back stack.
    enabled = false
    action = null
    hasLogicalParent = false
  }

  fun acceptsSource(source: String?, currentUrl: String?, mainFrame: Boolean): Boolean {
    val origin = applicationOrigin(source)
    return mainFrame && origin in origins && origin == applicationOrigin(currentUrl)
  }

  fun localState(message: String?, source: String?, currentUrl: String?, mainFrame: Boolean): Boolean {
    if (!mainFrame || message !in setOf("route", "overlay", "loading", "system")) return false
    if (!acceptsSource(source, currentUrl, mainFrame)) return false
    localDocument = true
    when (message) {
      "route" -> hasLogicalParent = true
      "system", "loading" -> hasLogicalParent = false
    }
    action = when {
      message == "overlay" -> "dismiss-overlay"
      message == "route" -> "navigate-back"
      else -> null
    }
    enabled = action != null
    return true
  }

  fun logicalParent(value: Boolean, source: String?, currentUrl: String?, mainFrame: Boolean): Boolean {
    if (!acceptsSource(source, currentUrl, mainFrame)) return false
    hasLogicalParent = value
    return true
  }
}
