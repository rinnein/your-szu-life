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

// HeroUI v3 / React Aria expose modal roles and positioned popover roots.
// Select/listbox menus live inside a popover; inline listboxes are not overlays.
export function activeOverlay(): HTMLElement | undefined {
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
    if (!disposed)
      bridge.postMessage(activeOverlay() ? "overlay" : options.canBack() ? "route" : "system");
  };
  bridge.onmessage = ({ data }) => {
    if ((data !== "dismiss-overlay" && data !== "navigate-back") || pending || disposed) return;
    const overlay = activeOverlay();
    if (overlay) {
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
  publish();
  return () => {
    disposed = true;
    observer.disconnect();
    unsubscribe();
    bridge.onmessage = null;
    // Do not hand an unmounted/loading page to the system as though it were home.
    bridge.postMessage("loading");
  };
}
