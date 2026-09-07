import type { ReactNode } from "react";
import appIcon from "../src-tauri/icons/icon.png";

export function PageShell({
  children,
  centered = false,
}: {
  children: ReactNode;
  centered?: boolean;
}) {
  return (
    <main className={centered ? "app-shell app-shell-centered" : "app-shell"}>{children}</main>
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
