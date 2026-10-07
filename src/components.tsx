import { LuArrowLeft } from "react-icons/lu";
import type { ReactNode } from "react";
import { Button, CloseButton } from "@heroui/react";
import { useLogicalNavigation } from "./logical-navigation";
import appIcon from "../src-tauri/icons/icon.png";

export function PageShell({
  children,
  centered = false,
  showAbout = false,
}: {
  children: ReactNode;
  centered?: boolean;
  showAbout?: boolean;
}) {
  const navigation = useLogicalNavigation();
  return (
    <main className={centered ? "app-shell app-shell-centered" : "app-shell"}>
      {children}
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
}: {
  title: string;
  description?: string;
  backLabel?: string;
  actions?: ReactNode;
}) {
  const nav = useLogicalNavigation();
  return (
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div className="flex min-w-0 items-center gap-3">
        <CloseButton data-webview-navigation aria-label={backLabel} onPress={() => void nav.back()}>
          <LuArrowLeft aria-hidden="true" />
        </CloseButton>
        <div>
          <h1 data-webview-navigation className="text-2xl font-semibold">
            {title}
          </h1>
          {description && <p className="text-sm text-muted">{description}</p>}
        </div>
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </header>
  );
}
