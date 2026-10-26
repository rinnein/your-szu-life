import type { ReactNode } from "react";
import { AppNavigation } from "./AppNavigation";

export function AppChrome({ children }: { children: ReactNode }) {
  return (
    <div className="app-frame">
      <AppNavigation />
      <main className="app-content">{children}</main>
    </div>
  );
}
