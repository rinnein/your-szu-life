import type { ReactNode } from "react";
import { useLogicalNavigation } from "../logical-navigation";
import { PageDrawerProvider } from "./PageDrawerContext";

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
    <PageDrawerProvider>
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
              onClick={() => void navigation.push("/settings?tab=about")}
            >
              关于 · 检查更新
            </button>
          </footer>
        )}
      </div>
    </PageDrawerProvider>
  );
}
