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

// material 的模态浮层都在 light DOM 宿主上反映 open 属性：dialog 恒为模态，
// 抽屉与 sheet 只有带 modal 时才算模态浮层（OJ 的 docked 侧栏就是非模态 Drawer）。
const MODAL_OVERLAYS = [
  "m-dialog[open]",
  "m-drawer[modal][open]",
  "m-bottom-sheet[modal][open]",
  "m-side-sheet[modal][open]",
  '[role="dialog"]',
  '[role="alertdialog"]',
  "[data-trigger][data-placement]",
  '[data-slot="select-popover"]',
].join(",");
const MATERIAL_OVERLAYS = "m-dialog, m-drawer, m-bottom-sheet, m-side-sheet";

function isModalOverlay(element: HTMLElement): boolean {
  switch (element.localName) {
    case "m-dialog":
    case "m-bottom-sheet":
    case "m-side-sheet":
      return element.hasAttribute("open");
    case "m-drawer":
      return element.hasAttribute("open") && element.hasAttribute("modal");
    default:
      return true;
  }
}

// 文档序最后一个是当前最上层：portal 按挂载顺序追加，嵌套 dialog 的内层在文档序更靠后。
// 只看 light DOM：material 的 dialog 会把 role="alertdialog" 放进 shadow root 内部，
// 那不是浮层宿主（生产环境的 document.querySelectorAll 本来也看不到它）。
export function activeOverlay(): HTMLElement | undefined {
  return Array.from(document.querySelectorAll<HTMLElement>(MODAL_OVERLAYS))
    .filter(
      (element) =>
        element.getRootNode() === document &&
        isModalOverlay(element) &&
        !element.closest('[aria-hidden="true"], [hidden], [data-exiting]'),
    )
    .pop();
}

function materialOverlayHost(overlay: HTMLElement): HTMLElement | undefined {
  if (overlay.localName.startsWith("m-")) return overlay;
  return overlay.querySelector<HTMLElement>(MATERIAL_OVERLAYS) ?? undefined;
}

/** 关闭 material 浮层；返回 false 表示不是 material 浮层，由调用方兜底。 */
function dismissOverlay(overlay: HTMLElement): boolean {
  const host = materialOverlayHost(overlay);
  if (!host) return false;
  if (host.localName === "m-drawer") {
    // drawer 宿主没有暴露 close()，移除 open 会走 attributeChangedCallback → close()。
    host.removeAttribute("open");
  } else {
    (host as { close?: () => void }).close?.();
  }
  return true;
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
      if (!dismissOverlay(overlay)) {
        // 非 material 的自绘浮层沿用它的 Escape 处理，绝不点击确认/危险按钮。
        const focused = document.activeElement;
        const target =
          focused instanceof HTMLElement && overlay.contains(focused) ? focused : overlay;
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
      }
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
    attributeFilter: ["aria-hidden", "hidden", "data-exiting", "role", "open", "modal"],
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
