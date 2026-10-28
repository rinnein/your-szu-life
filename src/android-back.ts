/** Android owns the system gesture; React only receives a committed back. */
export interface AndroidBackBridge {
  postMessage(message: string): void;
  onmessage: ((event: { data: string }) => void) | null;
}

declare global {
  interface Window {
    SzuApplicationBack?: AndroidBackBridge;
  }
}

/**
 * 向原生侧发送一条桥接消息（仅 Android 存在该桥）。
 *
 * 桥接协议（原生侧只接受本地主框架来源的消息，见 ApplicationBackNavigation）：
 * 前端 → 原生：`toolbar-query`、`parent:yes|no`、`back-request`、`document`、
 * `route|overlay|loading|system`、`chrome-status-bar:<#rrggbb>`、`chrome-nav-bar:<#rrggbb>`；
 * 原生 → 前端：`native-toolbar-ready`、`navigate-back`、`dismiss-overlay`、`wallpaper-changed`。
 *
 * `chrome-status-bar` 是顶部应用栏的当前颜色（滚动会变），`chrome-nav-bar` 是底部导航栏的颜色；
 * 原生侧据此给状态栏与「小白条」上色（桌面端没有该桥，调用方静默跳过）。
 */
export function postNativeMessage(message: string): boolean {
  const bridge = window.SzuApplicationBack;
  if (!bridge) return false;
  bridge.postMessage(message);
  return true;
}

// material 把模态对话框与浮层各自放进独立的顶层容器：
// m-dialog / m-bottom-sheet 有 open 属性，下拉菜单走 popover，行内列表框不算浮层。
export function activeOverlay(): HTMLElement | undefined {
  const mDialog = Array.from(document.querySelectorAll<HTMLElement>("m-dialog[open]")).pop();
  if (mDialog) return mDialog;
  const mSheet = Array.from(document.querySelectorAll<HTMLElement>("m-bottom-sheet[open]")).pop();
  if (mSheet) return mSheet;
  const overlays = Array.from(
    document.querySelectorAll<HTMLElement>(
      '[role="dialog"], [role="alertdialog"], [data-trigger][data-placement], [data-slot="select-popover"]',
    ),
  ).filter((element) => !element.closest('[aria-hidden="true"], [hidden], [data-exiting]'));
  return overlays[overlays.length - 1];
}

export function installAndroidBack(options: {
  canBack: () => boolean;
  goBack: () => Promise<unknown>;
  subscribe: (update: () => void) => () => void;
}): () => void {
  const bridge = window.SzuApplicationBack;
  if (!bridge) return () => {};
  let disposed = false;
  let pending = false;
  const publish = () => {
    if (!disposed) {
      bridge.postMessage(options.canBack() ? "parent:yes" : "parent:no");
      bridge.postMessage(activeOverlay() ? "overlay" : options.canBack() ? "route" : "system");
    }
  };
  bridge.onmessage = ({ data }) => {
    if (disposed) return;
    if (data === "native-toolbar-ready") {
      document.documentElement.dataset.szuNativeToolbar = "true";
      return;
    }
    // 壁纸换色没有系统广播，原生侧在 onResume 检测到变化后通知这里。
    if (data === "wallpaper-changed") {
      window.dispatchEvent(new CustomEvent("szu-wallpaper-changed"));
      return;
    }
    if ((data !== "dismiss-overlay" && data !== "navigate-back") || pending || disposed) return;
    const overlay = activeOverlay();
    if (overlay) {
      const root = overlay.getRootNode() as ShadowRoot | Document;
      const host = (root instanceof ShadowRoot ? root.host : null) ?? overlay;
      if (
        host instanceof HTMLElement &&
        (host.tagName.toLowerCase() === "m-dialog" ||
          host.tagName.toLowerCase() === "m-bottom-sheet")
      ) {
        (host as { close?: () => void }).close?.();
        host.dispatchEvent(new CustomEvent("close", { bubbles: true, composed: true }));
      } else {
        const custom = overlay.querySelector<HTMLElement>("m-dialog, m-bottom-sheet");
        if (custom) {
          (custom as { close?: () => void }).close?.();
          custom.dispatchEvent(new CustomEvent("close", { bubbles: true, composed: true }));
        }
      }
      const focused = document.activeElement;
      const target =
        focused instanceof HTMLElement && overlay.contains(focused) ? focused : overlay;
      // Use React Aria's existing Escape handling, including non-dismissable
      // dialogs. Never click a confirm/destructive button to close a dialog.
      target.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Escape",
          code: "Escape",
          bubbles: true,
          cancelable: true,
        }),
      );
      window.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Escape",
          code: "Escape",
          bubbles: true,
          cancelable: true,
        }),
      );
      publish();
      return;
    }
    // A disappearing overlay must not turn its back gesture into navigation.
    // Child Activities never receive navigate-back just for a non-root URL.
    if (data === "dismiss-overlay" || !options.canBack()) {
      publish();
      return;
    }
    pending = true;
    void Promise.resolve()
      .then(options.goBack)
      .catch(() => {
        // Stay on the page on navigation failure; never turn it into app exit.
      })
      .finally(() => {
        pending = false;
        publish();
      });
  };
  const observer = new MutationObserver(publish);
  observer.observe(document.body, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["aria-hidden", "hidden", "data-exiting", "role"],
  });
  const unsubscribe = options.subscribe(publish);
  bridge.postMessage("toolbar-query");
  publish();
  return () => {
    disposed = true;
    observer.disconnect();
    unsubscribe();
    bridge.onmessage = null;
    delete document.documentElement.dataset.szuNativeToolbar;
    // Do not hand an unmounted/loading page to the system as though it were home.
    bridge.postMessage("loading");
  };
}
