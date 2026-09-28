import { createMemoryHistory, createRouter } from "@tanstack/react-router";
import { initialLogicalEntries, isChildDocument } from "./logical-navigation";
import { routeTree } from "./routeTree.gen";

export const router = createRouter({
  routeTree,
  history: createMemoryHistory({ initialEntries: initialLogicalEntries(window.location.href) }),
  defaultPreload: "intent",
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
