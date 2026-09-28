import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useRouter } from "@tanstack/react-router";
import { supportsMultipleWindows } from "@tauri-apps/api/app";
import { getCurrentWebviewWindow, WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Alert } from "@heroui/react";

export const CHILD_PARAMETER = "__szu_window";
export function isChildDocument(href = window.location.href) {
  return new URL(href, "http://localhost").searchParams.get(CHILD_PARAMETER) === "child";
}
export function initialLogicalEntries(href: string) {
  const url = new URL(href, "http://localhost");
  const child = url.searchParams.get(CHILD_PARAMETER) === "child";
  url.searchParams.delete(CHILD_PARAMETER);
  const path = url.pathname + url.search + url.hash;
  return child || url.pathname === "/" ? [path] : ["/", path];
}
export function logicalBackAction(hasParent: boolean, child: boolean) {
  return hasParent ? "pop" : child ? "close" : "exit";
}
function nativeRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
export async function openLogicalWindow(path: string) {
  const url = new URL(path, window.location.origin);
  if (url.origin !== window.location.origin || !path.startsWith("/") || path.startsWith("//"))
    throw new Error("Invalid application route");
  url.searchParams.set(CHILD_PARAMETER, "child");
  const current = getCurrentWebviewWindow();
  const android = !!window.SzuApplicationBack;
  const parentActivity = android ? await current.activityName() : undefined;
  const child = new WebviewWindow(`feature-${crypto.randomUUID()}`, {
    url: url.pathname + url.search + url.hash,
    title: "泥深",
    ...(android ? { activityName: "FeatureActivity", createdByActivityName: parentActivity } : {}),
  });
  await new Promise<void>((resolve, reject) => {
    const subscriptions: Promise<() => void>[] = [];
    const finish = (error?: Error) => {
      clearTimeout(timer);
      for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten());
      if (error) reject(error);
      else resolve();
    };
    const timer = setTimeout(() => finish(new Error("Window creation timed out")), 15000);
    subscriptions.push(child.once("tauri://created", () => finish()));
    subscriptions.push(
      child.once("tauri://error", () => finish(new Error("Window creation failed"))),
    );
    for (const subscription of subscriptions)
      void subscription.catch(() => finish(new Error("Window event registration failed")));
  });
}

type Navigation = {
  push: (path: string) => Promise<void>;
  replace: (path: string) => Promise<void>;
  back: () => Promise<void>;
  canBack: () => boolean;
};
const Context = createContext<Navigation | null>(null);
export function LogicalNavigationProvider({ children }: { children: ReactNode }) {
  const router = useRouter();
  const child = isChildDocument();
  const opening = useRef(false);
  const [error, setError] = useState<string>();
  const back = useCallback(async () => {
    if (window.SzuApplicationBack) {
      // Native chooses pop/overlay/Activity close using the same state as the
      // system gesture. Do not await a Tauri close promise on Android.
      window.SzuApplicationBack.postMessage("back-request");
      return;
    }
    if (router.history.canGoBack()) router.history.back();
    else if (child && nativeRuntime()) await getCurrentWebviewWindow().destroy();
  }, [router, child]);
  const replace = useCallback(
    async (path: string) => {
      if (path === "/" && child && !router.history.canGoBack()) {
        await back();
        return;
      }
      await router.navigate({ to: path, replace: true });
    },
    [back, child, router],
  );
  const push = useCallback(
    async (path: string) => {
      if (opening.current) return;
      opening.current = true;
      setError(undefined);
      try {
        if (nativeRuntime() && window.SzuApplicationBack && (await supportsMultipleWindows())) {
          await openLogicalWindow(path);
        } else {
          await router.navigate({ to: path });
        }
      } catch {
        setError("无法打开独立页面，请重试；当前页面已保留。");
      } finally {
        opening.current = false;
      }
    },
    [router],
  );
  useEffect(() => {
    if (!nativeRuntime() || window.SzuApplicationBack) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebviewWindow()
      .onCloseRequested((event) => {
        // Desktop close and page back use the same logical-parent decision.
        if (router.history.canGoBack()) {
          event.preventDefault();
          void back();
        }
      })
      .then((value) => {
        if (disposed) value();
        else unlisten = value;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [back, router]);
  const value = useMemo(
    () => ({ push, replace, back, canBack: () => child || router.history.canGoBack() }),
    [push, replace, back, child, router],
  );
  return (
    <Context.Provider value={value}>
      {error && (
        <Alert status="danger" role="alert">
          <Alert.Content>
            <Alert.Title>页面未打开</Alert.Title>
            <Alert.Description>{error}</Alert.Description>
          </Alert.Content>
        </Alert>
      )}
      {children}
    </Context.Provider>
  );
}
export function useLogicalNavigation() {
  const value = useContext(Context);
  if (!value) throw new Error("LogicalNavigationProvider is missing");
  return value;
}
