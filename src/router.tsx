import { createRouter } from "@tanstack/react-router";
import { isChildDocument } from "./logical-navigation";
import { consumeAuthReturn, createReloadableLogicalHistory } from "./logical-history";
import { routeTree } from "./routeTree.gen";

function navigationStorage() {
  try {
    return window.sessionStorage;
  } catch {
    return undefined;
  }
}

export const router = createRouter({
  routeTree,
  history: createReloadableLogicalHistory({
    href: window.location.href,
    reload:
      (performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined)
        ?.type === "reload" || consumeAuthReturn(navigationStorage(), window.location.href),
    storage: navigationStorage(),
  }),
  defaultPreload: "intent",
  // 滚动位置由 LogicalNavigationProvider 按应用级逻辑栈精准恢复；关闭 TanStack 默认的整页 reset
  scrollRestoration: () => false,
});

// Reflect the logical location for native auth/portal validation, without ever
// adding a real WebView history entry. Child identity survives tab replacements.
const child = isChildDocument();
router.subscribe("onResolved", () => {
  const url = new URL(router.state.location.href, window.location.origin);
  if (child) url.searchParams.set("__szu_window", "child");
  window.history.replaceState(window.history.state, "", url.pathname + url.search + url.hash);
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
