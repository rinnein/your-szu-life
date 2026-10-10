import { LuArrowLeft, LuRefreshCw } from "react-icons/lu";
import type { ReactNode } from "react";
import { Button, CloseButton, Card as HeroCard, Spinner } from "@heroui/react";
import { useLogicalNavigation } from "./logical-navigation";
import appIcon from "../src-tauri/icons/icon.png";

export { SkeletonCard, SkeletonTable, SkeletonText } from "./components/Skeleton";
export { StatCard } from "./components/StatCard";

export function PageShell({
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
  const maxWidthClass = {
    sm: "max-w-screen-sm",
    md: "max-w-screen-md",
    lg: "max-w-screen-lg",
    xl: "max-w-screen-xl",
    "2xl": "max-w-screen-2xl",
    full: "max-w-full",
  }[maxWidth];

  return (
    <main className={centered ? "app-shell app-shell-centered" : "app-shell"}>
      <div className={`mx-auto w-full min-w-0 ${maxWidthClass}`}>{children}</div>
      {showAbout && (
        <footer className="app-footer">
          <Button variant="tertiary" onPress={() => void navigation.push("/about")}>
            关于 · 检查更新
          </Button>
        </footer>
      )}
    </main>
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

export function ModuleHeader({
  title,
  description,
  backLabel = "返回工作台",
  actions,
  eyebrow,
}: {
  title: string;
  description?: string;
  backLabel?: string;
  actions?: ReactNode;
  eyebrow?: string;
}) {
  const nav = useLogicalNavigation();
  return (
    <header className="flex flex-wrap items-start justify-between gap-4 mb-8">
      <div className="flex min-w-0 items-start gap-3 flex-1">
        <CloseButton
          data-webview-navigation
          aria-label={backLabel}
          onPress={() => void nav.back()}
          className="mt-1"
        >
          <LuArrowLeft aria-hidden="true" />
        </CloseButton>
        <div className="min-w-0 flex-1">
          {eyebrow && (
            <span
              data-webview-navigation
              className="block text-[11px] font-bold tracking-wider text-[#6971ae] mb-2.5"
            >
              {eyebrow}
            </span>
          )}
          <h1 data-webview-navigation className="text-2xl md:text-3xl font-semibold tracking-tight">
            {title}
          </h1>
          {description && (
            <p className="mt-2.5 text-sm leading-relaxed text-[var(--szu-muted)] max-w-[620px]">
              {description}
            </p>
          )}
        </div>
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </header>
  );
}

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
    <HeroCard
      className={`shadow-[0_12px_35px_#25285812] border border-[rgba(37,40,88,0.08)] ${className ?? ""}`}
      variant="default"
    >
      {(title || description || icon) && (
        <HeroCard.Header className="px-6 py-5 pb-3">
          {title && (
            <HeroCard.Title className="flex items-center gap-2.5">
              {icon && <span className="text-[var(--szu-brand)]">{icon}</span>}
              {title}
            </HeroCard.Title>
          )}
          {description && <HeroCard.Description>{description}</HeroCard.Description>}
        </HeroCard.Header>
      )}
      <HeroCard.Content className="px-6 py-4">{children}</HeroCard.Content>
      {actions && (
        <HeroCard.Footer className="px-6 py-4 pt-0 flex gap-2 flex-wrap">{actions}</HeroCard.Footer>
      )}
    </HeroCard>
  );
}

export function LoadingState({ message = "加载中…" }: { message?: string }) {
  return (
    <div
      className="flex items-center justify-center gap-3 py-12 text-[var(--szu-muted)]"
      role="status"
    >
      <Spinner size="md" />
      <span className="text-sm">{message}</span>
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
    <div className="flex flex-col items-center justify-center gap-3 py-12 text-center">
      {icon && (
        <div
          className={
            variant === "error"
              ? "text-red-600 opacity-80 mb-1"
              : "text-[var(--szu-muted)] opacity-60 mb-1"
          }
        >
          {icon}
        </div>
      )}
      <h3
        className={
          variant === "error" ? "font-semibold text-red-600" : "font-semibold text-[var(--szu-ink)]"
        }
      >
        {title}
      </h3>
      {description && <p className="text-sm text-[var(--szu-muted)] max-w-md">{description}</p>}
      {(action || onRetry) && (
        <div className="mt-2 flex gap-2">
          {onRetry && (
            <Button variant="secondary" onPress={onRetry}>
              <LuRefreshCw /> 重试
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
        <div className="flex items-start justify-between gap-4 mb-4">
          <div>
            {title && (
              <h2 id={`section-${title}`} className="text-xl font-semibold tracking-tight mb-1.5">
                {title}
              </h2>
            )}
            {description && (
              <p className="text-sm text-[var(--szu-muted)] leading-relaxed">{description}</p>
            )}
          </div>
          {actions && <div className="flex items-center gap-2 flex-wrap">{actions}</div>}
        </div>
      )}
      {children}
    </section>
  );
}
