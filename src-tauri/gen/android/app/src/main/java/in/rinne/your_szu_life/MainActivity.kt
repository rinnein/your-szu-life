package `in`.rinne.your_szu_life

import android.os.Bundle
import io.crates.keyring.Keyring

class MainActivity : ApplicationBackActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    Keyring.initializeNdkContext(applicationContext)
    super.onCreate(savedInstanceState)
    CampusNetworkBridge.bindActivity(this)
  }

  override fun onStart() {
    super.onStart()
    CampusNetworkBridge.bindActivity(this)
  }

  override fun onResume() {
    super.onResume()
    // Temporary diagnostic build; remove before shipping.
    android.webkit.WebView.setWebContentsDebuggingEnabled(true)
    if (androidx.webkit.WebViewFeature.isFeatureSupported(androidx.webkit.WebViewFeature.GET_COOKIE_INFO)) {
      for (url in listOf("https://authserver.szu.edu.cn/authserver/login", "https://authserver.szu.edu.cn/personalInfo/common/getUserConf")) {
        for (cookie in androidx.webkit.CookieManagerCompat.getCookieInfo(android.webkit.CookieManager.getInstance(), url)) {
          android.util.Log.i("SzuCookieAttributes", cookie.substringBefore('=') + "; " + cookie.substringAfter(';', ""))
        }
      }
    }
    CampusNetworkBridge.bindActivity(this)
  }

  override fun onDestroy() {
    CampusNetworkBridge.unbindActivity(this)
    super.onDestroy()
  }
}
