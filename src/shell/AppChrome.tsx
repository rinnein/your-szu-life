import { useCallback, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { AppNavigation } from "./AppNavigation";
import { FullscreenPageContext } from "./FullscreenPageContext";
import { PageTransition } from "../motion/PageTransition";
import { useNativeChromeColors } from "./native-chrome";

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
  const [fullscreenPages, setFullscreenPages] = useState(0);
  const registerFullscreenPage = useCallback(() => {
    setFullscreenPages((count) => count + 1);
    return () => setFullscreenPages((count) => count - 1);
  }, []);
  const fullscreen = fullscreenPages > 0;
  const contentRef = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const content = contentRef.current;
    if (!content) return;
    const measure = () => {
      content.style.setProperty(
        "--app-content-width",
        `${content.getBoundingClientRect().width}px`,
      );
    };
    measure();
    const observer = typeof ResizeObserver === "function" ? new ResizeObserver(measure) : null;
    observer?.observe(content);
    window.addEventListener("resize", measure);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, []);
  useLayoutEffect(() => {
    document.documentElement.dataset.appFullscreen = String(fullscreen);
    return () => {
      delete document.documentElement.dataset.appFullscreen;
    };
  }, [fullscreen]);
  // 沉浸式系统栏取色：状态栏跟随顶部应用栏的当前颜色，小白条取底部导航栏颜色。
  useNativeChromeColors();
  return (
    <FullscreenPageContext.Provider value={registerFullscreenPage}>
      <div className="app-frame" data-fullscreen={fullscreen || undefined}>
        {!fullscreen && <AppNavigation />}
        <main ref={contentRef} className="app-content">
          <PageTransition>{children}</PageTransition>
        </main>
      </div>
    </FullscreenPageContext.Provider>
  );
}
