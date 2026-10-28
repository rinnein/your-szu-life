import type { ReactNode } from "react";
import type { NavigateOptions } from "@tanstack/react-router";
import type { IconType } from "react-icons";
import { LuRefreshCw } from "react-icons/lu";
import { Card as UiCard, LoadingIndicator, Button } from "material/react";
import { useLogicalNavigation } from "./logical-navigation";
import { PageSurface } from "./shell/AppShell";
import { TopAppBar } from "./shell/TopAppBar";
import appIcon from "../src-tauri/icons/icon.png";

export { SkeletonCard, SkeletonTable, SkeletonText } from "./components/Skeleton";
export { StatCard } from "./components/StatCard";

/**
 * 页面外壳。
 *
 * 保留这个名字是因为 14 个页面都在用它；实现改为 M3 的 `PageSurface`。
 * 顶栏由各页面自行渲染 `TopAppBar`，不再由这里提供。
 */
export const PageShell = PageSurface;

/**
 * 模块页头。
 *
 * 保留这个名称是因为 14 个页面都在用它；实现是 M3 的 `TopAppBar`，
 * 并由 `TopAppBar` 统一提供命令面板搜索入口。
 * `backLabel` 沿用旧参数名，默认「返回上一页」。
 */
export function ModuleHeader({
  title,
  description,
  backLabel = "返回上一页",
  actions,
  eyebrow,
}: {
  title: string;
  description?: string;
  backLabel?: string;
  actions?: ReactNode;
  eyebrow?: string;
}) {
  return (
    <TopAppBar
      title={title}
      description={description}
      eyebrow={eyebrow}
      backLabel={backLabel}
      actions={actions}
    />
  );
}

export function BrandMark({ large = false }: { large?: boolean }) {
  return (
    <img
      className={large ? "brand-mark brand-mark-large" : "brand-mark"}
      src={appIcon}
      alt="泥深"
    />
  );
}

/**
 * 带标题/描述/操作区的卡片。
 *
 * 这是 `src/ui` 的 `Card` 之上的便捷包装：3 个页面直接用它，
 * 其余页面用 `ui/Card` 的复合写法。
 */
export function Card({
  title,
  description,
  icon,
  children,
  actions,
  className,
}: {
  title?: string;
  description?: string;
  icon?: ReactNode;
  children: ReactNode;
  actions?: ReactNode;
  className?: string;
}) {
  return (
    <UiCard className={className} variant="outlined" headline={title} subhead={description}>
      {icon && (
        <span slot="headline" className="card-title-icon mr-2 inline-flex items-center">
          {icon}
        </span>
      )}
      <div className="p-4">{children}</div>
      {actions && <div slot="actions">{actions}</div>}
    </UiCard>
  );
}

export function LoadingState({ message = "加载中…" }: { message?: string }) {
  return (
    <div className="empty-state" role="status">
      <LoadingIndicator size={24} />
      <span>{message}</span>
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
  variant = "default",
  onRetry,
}: {
  icon?: ReactNode;
  title: string;
  description?: string;
  action?: ReactNode;
  variant?: "default" | "error";
  onRetry?: () => void;
}) {
  return (
    <div className="empty-state" data-variant={variant}>
      {icon && (
        <div className={variant === "error" ? "empty-state-error" : "empty-state-icon"}>{icon}</div>
      )}
      <h3 className={variant === "error" ? "empty-state-error" : undefined}>{title}</h3>
      {description && <p className="empty-state-description">{description}</p>}
      {(action || onRetry) && (
        <div className="empty-state-actions">
          {onRetry && (
            <Button variant="tonal" onClick={onRetry}>
              <LuRefreshCw aria-hidden="true" />
              重试
            </Button>
          )}
          {action}
        </div>
      )}
    </div>
  );
}

export function Section({
  title,
  description,
  children,
  actions,
  className,
}: {
  title?: string;
  description?: string;
  children: ReactNode;
  actions?: ReactNode;
  className?: string;
}) {
  return (
    <section className={className} aria-labelledby={title ? `section-${title}` : undefined}>
      {(title || description || actions) && (
        <div className="section-header">
          <div>
            {title && (
              <h2 id={`section-${title}`} className="section-title">
                {title}
              </h2>
            )}
            {description && <p className="section-description">{description}</p>}
          </div>
          {actions && <div className="section-actions">{actions}</div>}
        </div>
      )}
      {children}
    </section>
  );
}

export type Feature = {
  label: string;
  icon: IconType;
  description: string;
  to?: NavigateOptions["to"];
};

/**
 * 功能入口网格，学习页与生活页共用。
 *
 * 富内容入口（图标 + 标题 + 描述）不适合 material 的纯文本按钮，用原生
 * button 承载 M3 卡片样式，文案天然可被读屏读取。没有 `to` 的入口（尚未
 * 实现的占位）交给 `onPlaceholder` 处理。
 */
export function FeatureGrid({
  features,
  onPlaceholder,
}: {
  features: readonly Feature[];
  onPlaceholder?: (label: string) => void;
}) {
  const navigation = useLogicalNavigation();
  return (
    <div className="feature-grid">
      {features.map(({ label, icon: FeatureIcon, description, to }) => (
        <button
          key={label}
          type="button"
          className="feature-button"
          onClick={() => (to ? void navigation.push(to) : onPlaceholder?.(label))}
        >
          <span className="feature-icon" aria-hidden="true">
            <FeatureIcon size={19} focusable="false" />
          </span>
          <span className="feature-copy">
            <strong>{label}</strong>
            <small>{description}</small>
          </span>
          <span className="feature-arrow" aria-hidden="true">
            →
          </span>
        </button>
      ))}
    </div>
  );
}
