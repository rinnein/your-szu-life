package `in`.rinne.your_szu_life

import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.ScriptHandler
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.json.JSONObject

/** No privileged JS interface, no replacement of Tauri's WebViewClient. */
class ApplicationBackNavigation(
  private val activity: ApplicationBackActivity,
  private val webView: WebView,
) {
  private val bridgeName = "SzuApplicationBack"
  private var reply: JavaScriptReplyProxy? = null
  private var capabilityReply: JavaScriptReplyProxy? = null
  private var disposed = false
  private var script: ScriptHandler? = null
  private val origins = mutableSetOf("http://tauri.localhost", "https://tauri.localhost")
  private val policy = ApplicationBackPolicy(origins)
  private val messaging = WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)
  private val documentStart = WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)
  private var lastUrl: String? = null

  private val callback = object : OnBackPressedCallback(false) {
    override fun handleOnBackPressed() {
      if (messaging && policy.localDocument) {
        // Reply proxies are document-bound: a navigation cannot deliver this
        // back to a different, possibly remote, document.
        val target = reply
        if (target != null && policy.action != null) {
          target.postMessage(policy.action!!)
        } else {
          // A failed/unfinished frontend must not trap the user in the app.
          isEnabled = false
          activity.onBackPressedDispatcher.onBackPressed()
        }
      } else {
        isEnabled = false
        activity.onBackPressedDispatcher.onBackPressed()
      }
    }
  }

  // Old WebView providers lack document-start injection. Only that fallback
  // polls the URL (not JS/credentials); current providers are event-driven.
  private val fallback = object : Runnable {
    override fun run() {
      if (disposed) return
      val url = webView.url
      if (url != lastUrl) {
        lastUrl = url
        reply = null
        capabilityReply = null
        policy.document(url)
        callback.isEnabled = policy.enabled
        activity.updateBrowserToolbar(policy.localDocument)
        activity.updateToolbarBack(policy.hasLogicalParent)
      }
      // No WebView history fallback: unknown state belongs to the system.
      if (!messaging) callback.isEnabled = false
      webView.postDelayed(this, 250)
    }
  }

  init {
    // Tauri writes the effective dev URL into the packaged config. Never allow
    // arbitrary remote origins or a wildcard to control local navigation.
    if (BuildConfig.DEBUG) {
      runCatching {
        val config = activity.assets.open("tauri.conf.json").bufferedReader().use { it.readText() }
        val url = JSONObject(config).optJSONObject("build")?.optString("devUrl")
        applicationOrigin(url)?.let { origins.add(it) }
      }
    }
    activity.onBackPressedDispatcher.addCallback(activity, callback)
    if (messaging) {
      // A document-start notice is needed for remote navigations too. Remote
      // pages can only refresh their own ordinary WebView back state; only
      // an exact app origin in the main frame can publish route/overlay/loading/system.
      WebViewCompat.addWebMessageListener(webView, bridgeName, setOf("*")) {
          _, message, sourceOrigin, isMainFrame, proxy ->
        if (!disposed && isMainFrame && message.type == WebMessageCompat.TYPE_STRING) {
          val data = message.data ?: ""
          when (data) {
            // Read-only capability: remote main frames may ask whether the native
            // bar is mounted, but cannot publish local navigation state.
            "toolbar-query" -> {
              capabilityReply = proxy
              toolbarMounted()
            }
            "parent:yes", "parent:no" -> if (policy.logicalParent(
                data == "parent:yes", sourceOrigin.toString(), webView.url, isMainFrame)) {
              activity.updateToolbarBack(policy.hasLogicalParent)
            }
            "back-request" -> if (policy.acceptsSource(sourceOrigin.toString(), webView.url, true)) {
              if (callback.isEnabled) callback.handleOnBackPressed()
              else activity.onBackPressedDispatcher.onBackPressed()
            }
            "document" -> {
              reply = null
              capabilityReply = null
              policy.document(sourceOrigin.toString())
              callback.isEnabled = policy.enabled
              activity.updateBrowserToolbar(policy.localDocument)
              activity.updateToolbarBack(policy.hasLogicalParent)
            }
            "route", "overlay", "loading", "system" -> if (policy.localState(
                data, sourceOrigin.toString(), webView.url, isMainFrame)) {
              reply = proxy
              lastUrl = webView.url
              callback.isEnabled = policy.enabled
              activity.updateToolbarBack(policy.hasLogicalParent)
            }
            // Web 端推来的系统栏颜色：状态栏取顶栏当前颜色（滚动会变），
            // 小白条取底部导航栏颜色。只有本地主框架能改窗口外观，与导航状态同一套来源校验。
            else -> if (policy.acceptsSource(sourceOrigin.toString(), webView.url, true)) {
              when {
                data.startsWith(CHROME_STATUS_BAR_PREFIX) -> activity.setChromeStatusBarColor(
                  parseCssColor(data.substring(CHROME_STATUS_BAR_PREFIX.length)))
                data.startsWith(CHROME_NAVIGATION_BAR_PREFIX) ->
                  activity.setChromeNavigationBarColor(
                    parseCssColor(data.substring(CHROME_NAVIGATION_BAR_PREFIX.length)))
              }
            }
          }
        }
      }
      if (documentStart) {
        script = WebViewCompat.addDocumentStartJavaScript(webView,
          "if (window === window.top) window.$bridgeName.postMessage('document');",
          setOf("*"))
      }
    }
    if (!messaging || !documentStart) webView.post(fallback)
  }

  fun toolbarMounted() {
    if (!disposed && activity.nativeToolbarReady) {
      activity.updateToolbarBack(policy.hasLogicalParent)
      capabilityReply?.postMessage("native-toolbar-ready")
    }
  }

  /** 壁纸变更后通知前端重新生成配色。复用同一条消息通道。 */
  fun notifyWallpaperChanged() {
    if (!disposed) capabilityReply?.postMessage("wallpaper-changed")
  }

  fun dispose() {
    disposed = true
    callback.remove()
    webView.removeCallbacks(fallback)
    script?.remove()
    if (messaging) WebViewCompat.removeWebMessageListener(webView, bridgeName)
    reply = null
    capabilityReply = null
  }
}
