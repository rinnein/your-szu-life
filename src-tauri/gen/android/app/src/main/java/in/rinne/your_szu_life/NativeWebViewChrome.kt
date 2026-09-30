package `in`.rinne.your_szu_life

import android.view.MenuItem
import android.view.ViewGroup
import android.webkit.WebView
import android.widget.LinearLayout
import androidx.appcompat.content.res.AppCompatResources
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.google.android.material.appbar.MaterialToolbar
import com.google.android.material.color.MaterialColors

/** Owns layout only: Wry retains its WebView, clients, IPC and lifecycle. */
internal class NativeWebViewChrome(
  private val activity: ApplicationBackActivity,
  private val webView: WebView,
) {
  val root = LinearLayout(activity).apply { orientation = LinearLayout.VERTICAL }
  private val toolbar = MaterialToolbar(activity)
  private var resumed = false
  private var disposed = false
  private val backIcon = AppCompatResources.getDrawable(activity, R.drawable.ic_browser_back)
  private val refreshIcon = AppCompatResources.getDrawable(activity, R.drawable.ic_browser_refresh)
  private val titleUpdate = object : Runnable {
    override fun run() {
      if (disposed || !resumed) return
      val title = browserTitle(webView.title)
      if (toolbar.title?.toString() != title) toolbar.title = title
      webView.postDelayed(this, 250)
    }
  }

  init {
    refreshTheme()
    toolbar.title = browserTitle(webView.title)
    toolbar.navigationContentDescription = activity.getString(R.string.browser_back)
    toolbar.setNavigationOnClickListener { activity.requestPageBack() }
    toolbar.menu.add(activity.getString(R.string.browser_refresh)).apply {
      icon = refreshIcon
      setShowAsAction(MenuItem.SHOW_AS_ACTION_ALWAYS)
      setOnMenuItemClickListener {
        webView.reload()
        true
      }
    }
    root.addView(toolbar, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
      (56 * activity.resources.displayMetrics.density).toInt()))
    (webView.parent as? ViewGroup)?.removeView(webView)
    root.addView(webView, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
    ViewCompat.setOnApplyWindowInsetsListener(root) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
      val cutout = insets.getInsets(WindowInsetsCompat.Type.displayCutout())
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(maxOf(bars.left, cutout.left), maxOf(bars.top, cutout.top),
        maxOf(bars.right, cutout.right), browserBottomInset(bars.bottom, cutout.bottom, ime.bottom))
      // The native container owns safe areas; descendants must not add them again.
      WindowInsetsCompat.CONSUMED
    }
    root.addOnAttachStateChangeListener(object : android.view.View.OnAttachStateChangeListener {
      override fun onViewAttachedToWindow(view: android.view.View) { ViewCompat.requestApplyInsets(view) }
      override fun onViewDetachedFromWindow(view: android.view.View) {}
    })
  }

  fun refreshTheme() {
    val surface = MaterialColors.getColor(toolbar, com.google.android.material.R.attr.colorSurface)
    val foreground = MaterialColors.getColor(toolbar, com.google.android.material.R.attr.colorOnSurface)
    root.setBackgroundColor(surface)
    toolbar.setBackgroundColor(surface)
    toolbar.setTitleTextColor(foreground)
    backIcon?.setTint(foreground)
    refreshIcon?.setTint(foreground)
    ViewCompat.requestApplyInsets(root)
  }

  fun showBack(visible: Boolean) { toolbar.navigationIcon = if (visible) backIcon else null }

  fun setResumed(value: Boolean) {
    resumed = value
    webView.removeCallbacks(titleUpdate)
    if (value && !disposed) webView.post(titleUpdate)
  }

  fun dispose() {
    disposed = true
    webView.removeCallbacks(titleUpdate)
    ViewCompat.setOnApplyWindowInsetsListener(root, null)
    toolbar.setNavigationOnClickListener(null)
    toolbar.menu.clear()
  }
}
