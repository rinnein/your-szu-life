package `in`.rinne.your_szu_life

import android.view.Window
import androidx.core.view.WindowInsetsControllerCompat
import com.google.android.material.color.MaterialColors

/**
 * 状态栏与「小白条」（手势导航条）共用同一套沉浸式逻辑：两条系统栏在同一个入口上色，
 * 图标明暗也一起决定；内容区留白由 [NativeWebViewChrome] 的 insets 监听统一处理，
 * 因此两条栏上的颜色都来自应用自己绘制的那条带状区域。
 *
 * 取色规则（见 [statusBarColorFor]、[navigationBarColorFor]）：
 * - 状态栏取「顶部条」当前的颜色——外部页面是原生浏览器栏，本地页面是 Web 顶栏推来的
 *   颜色（滚动时顶栏会从 surface 变成 surface-container，状态栏跟着变）。
 * - 小白条固定取底部导航栏的颜色，不随滚动变化。
 *
 * Android 15 起 `statusBarColor` / `navigationBarColor` 被系统忽略，此时这两条区域由
 * 应用的根容器绘制；色值仍然通过同一个入口下发，行为不变。
 */
internal object SystemBars {
  private const val FALLBACK_COLOR = 0xFF000000.toInt()

  /** 主题的 surface 角色色：还没有 Web 端推来的颜色时用它。 */
  fun themeSurface(window: Window): Int =
    MaterialColors.getColor(
      window.context,
      com.google.android.material.R.attr.colorSurface,
      FALLBACK_COLOR,
    )

  /** 主题的 surface-container 角色色：底部导航栏用的那一档。 */
  fun themeSurfaceContainer(window: Window): Int =
    MaterialColors.getColor(
      window.context,
      com.google.android.material.R.attr.colorSurfaceContainer,
      themeSurface(window),
    )

  fun apply(window: Window?, statusBar: Int, navigationBar: Int) {
    val target = window ?: return
    @Suppress("DEPRECATION") // Android 15 起被忽略，届时由根容器绘制该区域。
    target.statusBarColor = statusBar
    @Suppress("DEPRECATION")
    target.navigationBarColor = navigationBar
    val statusLight = isLightSurface(statusBar)
    val navigationLight = isLightSurface(navigationBar)
    WindowInsetsControllerCompat(target, target.decorView).apply {
      isAppearanceLightStatusBars = statusLight
      isAppearanceLightNavigationBars = navigationLight
    }
  }
}