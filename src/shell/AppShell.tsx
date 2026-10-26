import type { ReactNode } from "react";
import { useLogicalNavigation } from "../logical-navigation";
import { AppNavigation } from "./AppNavigation";
import { PageTransition } from "../motion/PageTransition";

/**
 * 应用外壳：导航 + 内容区。
 *
 * 布局约定：
 * - 窄屏：底部 NavigationBar，内容区预留底部安全区。
 * - 宽屏：左侧 NavigationRail，内容区在其右侧。
 *
 * 不在这里渲染顶栏 —— 顶栏由各页面的 `TopAppBar` 提供，
 * 因为工作台、独立门户页与 tab 页的顶栏结构不同。
 */
export function AppShell({ children }: { children: ReactNode }) {
  return (
    <div className="app-frame">
      <AppNavigation />
      <main className="app-content">
        <PageTransition>{children}</PageTransition>
      </main>
    </div>
  );
}

/** 页面内容的容器，替代原来的 `PageShell` 外层。 */
export function PageSurface({
  children,
  centered = false,
  showAbout = false,
  maxWidth = "full",
}: {
  children: ReactNode;
  centered?: boolean;
  showAbout?: boolean;
  maxWidth?: "sm" | "md" | "lg" | "xl" | "2xl" | "full";
}) {
  const navigation = useLogicalNavigation();
  const width = {
    sm: "42rem",
    md: "48rem",
    lg: "64rem",
    xl: "80rem",
    "2xl": "96rem",
    full: "100%",
  }[maxWidth];

  return (
    <div
      className={centered ? "page-surface page-surface-centered" : "page-surface"}
      // 居中的认证流程是全屏独立页面，不应显示底部导航栏。
      data-fullscreen={centered || undefined}
    >
      <div className="page-surface-inner" style={{ maxInlineSize: width }}>
        {children}
      </div>
      {showAbout && (
        <footer className="page-surface-footer">
          <button
            type="button"
            className="page-surface-footer-link"
            onClick={() => void navigation.push("/about")}
          >
            关于 · 检查更新
          </button>
        </footer>
      )}
    </div>
  );
}
