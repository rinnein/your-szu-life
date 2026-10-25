import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";

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

  const navigate = (url: string) => {
    const route = parseDeepLinkUrl(url);
    if (route) {
      onNavigate(route);
    }
  };

  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    // 启动参数和运行期事件可能递送同一个 URL，这期间去重避免多压一层逻辑栈。
    const handledDuringStartup = new Set<string>();
    let drainingStartup = true;
    const handleEventUrl = (url: string) => {
      if (drainingStartup) handledDuringStartup.add(url);
      navigate(url);
    };

    try {
      const unlistenDeepLink = await onOpenUrl((urls) => {
        for (const url of urls) {
          handleEventUrl(url);
        }
      });
      if (typeof unlistenDeepLink === "function") {
        unlistens.push(unlistenDeepLink);
      }
    } catch {
      // Deep link plugin listener registration failed or unavailable
    }

    // 先注册监听再读启动参数：链接若在监听就绪前送达，事件会丢失，但
    // 插件已把它写入 current，这里仍能补回。
    try {
      const startupUrls = await getCurrent();
      for (const url of startupUrls ?? []) {
        if (!handledDuringStartup.has(url)) {
          navigate(url);
        }
      }
    } catch {
      // Deep link startup arguments unavailable
    } finally {
      drainingStartup = false;
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
