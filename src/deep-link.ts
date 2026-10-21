import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onOpenUrl } from "@tauri-apps/plugin-deep-link";

export function parseDeepLinkUrl(urlStr: string): string | null {
  const trimmed = urlStr.trim();
  let route = "";
  if (trimmed.startsWith("your-szu-life://page")) {
    route = trimmed.slice("your-szu-life://page".length);
  } else if (trimmed.startsWith("your-szu-life://")) {
    route = trimmed.slice("your-szu-life://".length);
  } else {
    return null;
  }
  const normalized = "/" + route.replace(/^\/+/, "");
  return normalized;
}

export async function setupDeepLinkListener(
  onNavigate: (route: string) => void,
): Promise<() => void> {
  const unlistens: Array<() => void> = [];

  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    try {
      const unlistenDeepLink = await onOpenUrl((urls) => {
        for (const url of urls) {
          const route = parseDeepLinkUrl(url);
          if (route) {
            onNavigate(route);
          }
        }
      });
      if (typeof unlistenDeepLink === "function") {
        unlistens.push(unlistenDeepLink);
      }
    } catch {
      // Deep link plugin listener registration failed or unavailable
    }

    try {
      const unlistenMcpNavigate: UnlistenFn = await listen<{ route: string; uri: string }>(
        "navigate-page",
        (event) => {
          if (event.payload?.route) {
            onNavigate(event.payload.route);
          }
        },
      );
      unlistens.push(unlistenMcpNavigate);
    } catch {
      // Event listener registration failed
    }
  }

  return () => {
    for (const unlisten of unlistens) {
      unlisten();
    }
  };
}
