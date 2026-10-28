import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  type ReactNode,
} from "react";
import { useRouter } from "@tanstack/react-router";
import { supportsMultipleWindows } from "@tauri-apps/api/app";
import { getCurrentWebviewWindow, WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { toast } from "./ui";

export const CHILD_PARAMETER = "__szu_window";
export function isChildDocument(href = window.location.href) {
  return new URL(href, "http://localhost").searchParams.get(CHILD_PARAMETER) === "child";
}
/**
 * 深链接进入时补齐的逻辑父链。
 *
 * 路径按归属部件嵌套（`/study/uooc`、`/life/campus-payments/electricity`），
 * 因此父链直接由 URL 层级推出：首页 → 部件页 → 中间布局页 → 当前页。
 * 只有两处中间层级需要在 URL 之外补出来：选课页（登录页的 returnTo 与
 * view=cache 都挂在选课页下）与校园卡页。
 */
export function initialLogicalEntries(href: string) {
  const url = new URL(href, "http://localhost");
  const child = url.searchParams.get(CHILD_PARAMETER) === "child";
  url.searchParams.delete(CHILD_PARAMETER);
  const path = url.pathname + url.search + url.hash;
  if (child || url.pathname === "/") return [path];
  const pathname = url.pathname.replace(/\/+$/, "") || "/";
  const hub = pathname.split("/").filter(Boolean)[0];
  if (!hub) return ["/", path];
  const hubPath = `/${hub}`;
  const enrollmentPage = "/study/enrollment";
  const enrollmentChildren = [
    `${enrollmentPage}/selected`,
    `${enrollmentPage}/tasks`,
    `${enrollmentPage}/timetable`,
    `${enrollmentPage}/settings`,
  ];
  const legacyTabChild =
    pathname === enrollmentPage &&
    ["selected", "tasks", "timetable"].includes(url.searchParams.get("tab") ?? "");
  const loginChild =
    pathname === `${enrollmentPage}/login` &&
    enrollmentChildren.includes((url.searchParams.get("returnTo") ?? "").split("?")[0]);
  const cachedParent =
    url.searchParams.get("view") === "cache" ||
    (loginChild &&
      new URLSearchParams((url.searchParams.get("returnTo") ?? "").split("?")[1]).get("view") ===
        "cache");
  if (enrollmentChildren.includes(pathname) || legacyTabChild || loginChild)
    return ["/", hubPath, cachedParent ? `${enrollmentPage}?view=cache` : enrollmentPage, path];
  if (pathname.startsWith("/life/campus-payments/"))
    return ["/", hubPath, "/life/campus-payments", path];
  if (pathname === hubPath) return ["/", path];
  return ["/", hubPath, path];
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

type NavigationAction = "push" | "replace" | "back";

type Navigation = {
  push: (path: string) => Promise<void>;
  replace: (path: string) => Promise<void>;
  back: () => Promise<void>;
  canBack: () => boolean;
  /**
   * 读取最近一次导航动作。用函数而不是值：若把它放进 context 值，
   * 每次导航都会产生新的 context 对象，所有消费 `useNavigation` 的
   * useEffect 都会重跑 —— CampusPaymentsPage 的认证守卫因此在返回时
   * 多调一次 back()，把工作台也弹掉。
   */
  lastAction: () => NavigationAction | null;
};
const Context = createContext<Navigation | null>(null);
export function LogicalNavigationProvider({ children }: { children: ReactNode }) {
  const router = useRouter();
  const child = isChildDocument();
  const opening = useRef(false);
  const lastActionRef = useRef<NavigationAction | null>(null);
  const record = useCallback((action: NavigationAction) => {
    lastActionRef.current = action;
  }, []);
  const readLastAction = useCallback(() => lastActionRef.current, []);
  const back = useCallback(async () => {
    if (window.SzuApplicationBack) {
      record("back");
      // Native chooses pop/overlay/Activity close using the same state as the
      // system gesture. Do not await a Tauri close promise on Android.
      window.SzuApplicationBack.postMessage("back-request");
      return;
    }
    // 先记录动作再导航：`record` 只写 ref、不触发渲染，因此不会在导航中途
    // 重挂载订阅树（那会打断 history 操作）；反过来若在导航之后再记录，
    // 渲染这次跳转的那一帧读到的还是上一个动作，顶栏的入场方向会错一档。
    if (router.history.canGoBack()) {
      record("back");
      router.history.back();
    } else if (child && nativeRuntime()) await getCurrentWebviewWindow().destroy();
  }, [child, record, router]);
  const replace = useCallback(
    async (path: string) => {
      if (path === "/" && child && !router.history.canGoBack()) {
        await back();
        return;
      }
      record("replace");
      await router.navigate({ to: path, replace: true });
    },
    [back, child, record, router],
  );
  const push = useCallback(
    async (path: string) => {
      if (opening.current) return;
      opening.current = true;
      record("push");
      try {
        if (nativeRuntime() && window.SzuApplicationBack && (await supportsMultipleWindows())) {
          await openLogicalWindow(path);
        } else {
          await router.navigate({ to: path });
        }
      } catch {
        toast.danger("页面未打开", {
          description: "无法打开独立页面，请重试；当前页面已保留。",
        });
      } finally {
        opening.current = false;
      }
    },
    [record, router],
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
    () => ({
      push,
      replace,
      back,
      canBack: () => child || router.history.canGoBack(),
      lastAction: readLastAction,
    }),
    [push, replace, back, child, router, readLastAction],
  );
  return <Context.Provider value={value}>{children}</Context.Provider>;
}
export function useLogicalNavigation() {
  const value = useContext(Context);
  if (!value) throw new Error("LogicalNavigationProvider is missing");
  return value;
}
export { Context as LogicalNavigationContext };
