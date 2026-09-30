package `in`.rinne.your_szu_life

import android.view.ViewGroup
import android.view.WindowManager
import androidx.activity.ComponentDialog
import androidx.activity.OnBackPressedCallback
import androidx.core.view.WindowCompat
import android.os.Handler
import android.os.Looper
import java.lang.ref.WeakReference
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** Native window ownership only; never carries credentials or cookie values. */
object AuthWindowBridge {
  private var main = WeakReference<ApplicationBackActivity>(null)
  private var foreground = WeakReference<ApplicationBackActivity>(null)
  private var previous = WeakReference<ApplicationBackActivity>(null)
  private var active = false
  private var presentation: ComponentDialog? = null
  private var restoreContent: (() -> Unit)? = null

  fun created(activity: ApplicationBackActivity) {
    if (activity is MainActivity) main = WeakReference(activity)
  }

  fun resumed(activity: ApplicationBackActivity) { foreground = WeakReference(activity) }
  fun paused(activity: ApplicationBackActivity) {
    if (foreground.get() === activity) foreground.clear()
  }

  fun destroyed(activity: ApplicationBackActivity) {
    paused(activity)
    if (previous.get() === activity) {
      main.get()?.returnFromAuthentication()
      finishAuthentication(false)
      previous.clear()
    }
    if (main.get() === activity) { main.clear(); active = false; previous.clear() }
  }

  private fun onMain(action: () -> Boolean): Boolean {
    if (Looper.myLooper() == Looper.getMainLooper()) return action()
    val result = AtomicBoolean(false)
    val cancelled = AtomicBoolean(false)
    val done = CountDownLatch(1)
    Handler(Looper.getMainLooper()).post {
      try { if (!cancelled.get()) result.set(runCatching(action).getOrDefault(false)) } finally { done.countDown() }
    }
    if (!done.await(3, TimeUnit.SECONDS)) { cancelled.set(true); return false }
    return result.get()
  }

  fun showAuthentication(returnUrl: String): Boolean = onMain {
    val target = main.get()?.takeUnless { it.isFinishing || it.isDestroyed } ?: return@onMain false
    if (!active) previous = WeakReference(foreground.get()?.takeUnless { it === target })
    active = true
    target.setAuthReturnUrl(returnUrl)
    val source = foreground.get() ?: target
    if (source !== target && presentation == null) {
      val content = target.authenticationContent ?: return@onMain false
      val parent = content.parent as? ViewGroup ?: return@onMain false
      val index = parent.indexOfChild(content)
      val params = content.layoutParams
      val dialog = ComponentDialog(source, R.style.Theme_your_szu_life)
      restoreContent = {
        (content.parent as? ViewGroup)?.removeView(content)
        if (!target.isDestroyed) parent.addView(content, index.coerceAtMost(parent.childCount), params)
        target.setAuthenticationPresented(false)
      }
      try {
        parent.removeView(content)
        dialog.setContentView(content)
        dialog.setCanceledOnTouchOutside(false)
        dialog.onBackPressedDispatcher.addCallback(object : OnBackPressedCallback(true) {
          override fun handleOnBackPressed() { target.returnFromAuthentication() }
        })
        dialog.setOnCancelListener { target.returnFromAuthentication() }
        dialog.setOnDismissListener {
          restoreContent?.invoke()
          restoreContent = null
          presentation = null
        }
        presentation = dialog
        dialog.show()
        dialog.window?.let { window ->
          WindowCompat.setDecorFitsSystemWindows(window, false)
          window.setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
          window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
        }
        target.setAuthenticationPresented(true)
      } catch (_: Exception) {
        dialog.dismiss()
        restoreContent?.invoke()
        restoreContent = null
        presentation = null
        target.setAuthReturnUrl(null)
        active = false
        return@onMain false
      }
    }
    true
  }

  fun finishAuthentication(restore: Boolean): Boolean = onMain {
    val target = main.get()
    target?.setAuthReturnUrl(null)
    val source = previous.get()?.takeUnless { it.isFinishing || it.isDestroyed }
    previous.clear()
    // Borrow only the existing main content View. The Activity stack and
    // original feature WebView never move, including nested FeatureActivities.
    presentation?.dismiss()
    restoreContent?.invoke()
    restoreContent = null
    presentation = null
    if (active && restore && source != null) source.window.decorView.requestFocus()
    active = false
    true
  }
}
