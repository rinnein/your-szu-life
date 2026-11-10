package `in`.rinne.your_szu_life

/** Authentication window lifecycle, separate from shared Cookie transport. */
object AuthCookieBridge {
  @JvmStatic
  fun showAuthentication(returnUrl: String): Boolean = AuthWindowBridge.showAuthentication(returnUrl)

  @JvmStatic
  fun finishAuthentication(restore: Boolean): Boolean = AuthWindowBridge.finishAuthentication(restore)
}
