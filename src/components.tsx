import type { ReactNode } from "react";
import { Button } from "@heroui/react";
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
