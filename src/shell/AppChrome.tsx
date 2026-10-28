import type { ReactNode } from "react";
import { AppNavigation } from "./AppNavigation";
import { useNativeChromeColors } from "./native-chrome";

export function AppChrome({ children }: { children: ReactNode }) {
  // 沉浸式系统栏取色：状态栏跟随顶部应用栏的当前颜色，小白条取底部导航栏颜色。
  useNativeChromeColors();
  return (
    <div className="app-frame">
      <AppNavigation />
      <main className="app-content">{children}</main>
    </div>
  );
}
