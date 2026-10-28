package `in`.rinne.your_szu_life

import android.view.MenuItem
import android.view.View
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
  /**
   * 上下两条系统栏区域各由一块占位视图绘制。
   *
   * Android 15 起 `window.statusBarColor` / `navigationBarColor` 被系统忽略，边到边模式下
   * 这两条带状区域只能由应用自己画；`root` 的背景只有一种颜色，没法同时当状态栏与小白条，
   * 所以按 inset 高度各自上色。旧版本系统会用自己的不透明系统栏盖住这两块，效果一致。
   */
  private val statusBarBand = View(activity)
  private val navigationBarBand = View(activity)
  private var resumed = false
  private var disposed = false
  private var statusBarColor = 0
  private var navigationBarColor = 0
  private val backIcon = AppCompatResources.getDrawable(activity, R.drawable.ic_browser_back)
  private val refreshIcon = AppCompatResources.getDrawable(activity, R.drawable.ic_browser_refresh)

  /** 原生浏览器栏是否可见：只有外部（非本地）页面才需要它。 */
  var browserToolbarVisible = false
    private set

  /** 原生浏览器栏自身的颜色：外部页面的顶部条与状态栏都取它（跟随壁纸派生主题）。 */
  val surfaceColor: Int
    get() = MaterialColors.getColor(toolbar, com.google.android.material.R.attr.colorSurface)

  private val titleUpdate = object : Runnable {
    override fun run() {
      if (disposed || !resumed || !browserToolbarVisible) return
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
    root.addView(statusBarBand, bandParams())
    root.addView(toolbar, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
      (56 * activity.resources.displayMetrics.density).toInt()))
    (webView.parent as? ViewGroup)?.removeView(webView)
    root.addView(webView, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
    root.addView(navigationBarBand, bandParams())
    // 初始隐藏：等待首个 document 通知判定本地/外部页面，本地页面上不会闪出原生栏。
    toolbar.visibility = View.GONE
    ViewCompat.setOnApplyWindowInsetsListener(root) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
      val cutout = insets.getInsets(WindowInsetsCompat.Type.displayCutout())
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      // 左右留白与键盘留给容器；上下两条系统栏区域交给各自的颜色块。
      view.setPadding(maxOf(bars.left, cutout.left), 0, maxOf(bars.right, cutout.right), ime.bottom)
      setBandHeight(statusBarBand, maxOf(bars.top, cutout.top))
      // 键盘弹出时导航栏区域被键盘盖住，颜色块收成 0，避免在键盘上方留一条色带。
      setBandHeight(
        navigationBarBand,
        if (ime.bottom > 0) 0 else maxOf(bars.bottom, cutout.bottom),
      )
      // The native container owns safe areas; descendants must not add them again.
      WindowInsetsCompat.CONSUMED
    }
    root.addOnAttachStateChangeListener(object : android.view.View.OnAttachStateChangeListener {
      override fun onViewAttachedToWindow(view: android.view.View) { ViewCompat.requestApplyInsets(view) }
      override fun onViewDetachedFromWindow(view: android.view.View) {}
    })
  }

  private fun bandParams() =
    LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0)

  private fun setBandHeight(band: View, height: Int) {
    if (band.layoutParams?.height == height) return
    band.layoutParams = (band.layoutParams ?: bandParams()).also { it.height = height }
  }

  /** 系统栏取色的唯一入口：状态栏块与小白条块分别上色（由 Activity 解析后下发）。 */
  fun setSystemBarColors(statusBar: Int, navigationBar: Int) {
    statusBarColor = statusBar
    navigationBarColor = navigationBar
    statusBarBand.setBackgroundColor(statusBar)
    navigationBarBand.setBackgroundColor(navigationBar)
  }

  fun refreshTheme() {
    val surface = surfaceColor
    val foreground = MaterialColors.getColor(toolbar, com.google.android.material.R.attr.colorOnSurface)
    root.setBackgroundColor(surface)
    toolbar.setBackgroundColor(surface)
    toolbar.setTitleTextColor(foreground)
    backIcon?.setTint(foreground)
    refreshIcon?.setTint(foreground)
    ViewCompat.requestApplyInsets(root)
  }

  /**
   * 本地页面用 Web 顶栏，外部页面才显示原生栏。
   *
   * 隐藏时彻底移出布局，WebView 占满剩余空间；返回入口由可见性统一决定
   * ——原生栏可见即代表这是一张外部页面，必须给出离开它的方式。
   */
  fun setBrowserToolbarVisible(visible: Boolean) {
    if (browserToolbarVisible == visible) return
    browserToolbarVisible = visible
    toolbar.visibility = if (visible) View.VISIBLE else View.GONE
    showBack(visible)
    webView.removeCallbacks(titleUpdate)
    if (visible) {
      toolbar.title = browserTitle(webView.title)
      if (resumed && !disposed) webView.post(titleUpdate)
    } else {
      ViewCompat.requestApplyInsets(root)
    }
  }

  fun showBack(visible: Boolean) { toolbar.navigationIcon = if (visible) backIcon else null }

  fun setResumed(value: Boolean) {
    resumed = value
    webView.removeCallbacks(titleUpdate)
    if (value && !disposed && browserToolbarVisible) webView.post(titleUpdate)
  }

  fun dispose() {
    disposed = true
    webView.removeCallbacks(titleUpdate)
    ViewCompat.setOnApplyWindowInsetsListener(root, null)
    toolbar.setNavigationOnClickListener(null)
    toolbar.menu.clear()
  }
}
