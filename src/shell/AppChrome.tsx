import { useEffect, type ReactNode } from "react";
import { startChildWindowTitleSync } from "./child-window-title";
import { AppNavigation } from "./AppNavigation";
import { PageTransition } from "../motion/PageTransition";
import { useNativeChromeColors } from "./native-chrome";
import { isChildDocument, nativeRuntime } from "../logical-navigation";

/**
 * 应用外壳：导航 + 转场 + 内容区。
 *
 * 转场只在建立/回退页面层级时播放（push / back）；同一逻辑页面内的
 * tab、筛选器切换走 `replace`，`PageTransition` 据此不播放动画
 * （见 src/motion/PageTransition.tsx）。
 *
 * 顶栏不在这里渲染 —— 顶栏由各页面的 `TopAppBar` 提供，因为工作台、
 * 独立门户页与 tab 页的顶栏结构不同。
 */
export function AppChrome({ children }: { children: ReactNode }) {
  // 沉浸式系统栏取色：状态栏跟随顶部应用栏的当前颜色，小白条取底部导航栏颜色。
  useNativeChromeColors();
  // 子窗口标题与路由保持一致（仅桌面 feature 窗口生效）。
  useEffect(() => startChildWindowTitleSync(), []);
  // 桌面的 pin/feature 子窗口是独立页面，不渲染全局导航（Android 的
  // FeatureActivity 同样带 child 参数，但保持底栏不变）。
  const desktopChild = isChildDocument() && nativeRuntime() && !window.SzuApplicationBack;
  return (
    <div className="app-frame" data-child-window={desktopChild || undefined}>
      {!desktopChild && <AppNavigation />}
      <main className="app-content">
        <PageTransition>{children}</PageTransition>
      </main>
    </div>
  );
}
