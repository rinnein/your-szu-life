import type { ReactNode } from "react";

export type BannerStatus = "default" | "accent" | "success" | "warning" | "danger" | "info";

/**
 * 行内横幅：material 没有对应组件，自建。
 *
 * material 没有对应组件（它的 Snackbar 是瞬时的、Dialog 是模态的），
 * 而应用里有 29 个文件用它展示持久性说明与错误，因此自建。
 * `status` 决定状态配色的语义角色。
 */
export function Banner({
  status = "default",
  children,
  className,
  role,
}: {
  status?: BannerStatus;
  children: ReactNode;
  className?: string;
  role?: string;
}) {
  // M3 用 container/on-container 成对角色表达状态色，保证对比度。
  const palette: Record<BannerStatus, string> = {
    default: "banner-default",
    accent: "banner-accent",
    success: "banner-success",
    warning: "banner-warning",
    danger: "banner-danger",
    info: "banner-info",
  };
  return (
    <div
      className={`banner ${palette[status]} ${className ?? ""}`}
      role={role ?? (status === "danger" ? "alert" : "status")}
    >
      {children}
    </div>
  );
}

function BannerTitle({ children, className }: { children: ReactNode; className?: string }) {
  return <p className={`banner-title ${className ?? ""}`}>{children}</p>;
}

function BannerDescription({ children, className }: { children: ReactNode; className?: string }) {
  return <p className={`banner-description ${className ?? ""}`}>{children}</p>;
}

Banner.Title = BannerTitle;
Banner.Description = BannerDescription;
