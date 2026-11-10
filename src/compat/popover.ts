interface Portal {
  owner: Element;
  host: HTMLDivElement;
  restore: () => void;
}

/** 原生 top layer 无法 polyfill：只搬移浮层，保留原控件与触发按钮。 */
export function installPopoverPortals(injectStyles: (root: ShadowRoot) => void): () => void {
  const portals = new Map<HTMLElement, Portal>();
  const originalShow = HTMLElement.prototype.showPopover;
  const originalHide = HTMLElement.prototype.hidePopover;
  const originalToggle = HTMLElement.prototype.togglePopover;
  function restore(element: HTMLElement) {
    portals.get(element)?.restore();
  }
  function move(element: HTMLElement) {
    if (portals.has(element)) return;
    const origin = element.getRootNode();
    if (!(origin instanceof ShadowRoot) || !element.parentNode) return;
    const owner = origin.host;
    let context: Node | null = element.parentNode;
    let constrained = false;
    while (context && context !== document.body) {
      if (context instanceof Element) {
        const style = getComputedStyle(context);
        if (
          [style.overflowX, style.overflowY].some((value) => value !== "visible") ||
          style.transform !== "none" ||
          style.perspective !== "none" ||
          style.filter !== "none" ||
          /layout|paint|strict|content/.test(style.contain) ||
          (context !== owner && style.zIndex !== "auto" && style.position !== "static")
        )
          constrained = true;
      }
      context = context instanceof ShadowRoot ? context.host : context.parentNode;
    }
    // 已经直接处在 body 的轻提示等保留原 Shadow Root，保留 ::part 外观和定位。
    if (!constrained) return;
    const placeholder = document.createComment("szu-popover-return");
    element.before(placeholder);
    const host = document.createElement("div");
    host.className = owner.className;
    host.dataset.szuPopoverPortal = "";
    Object.assign(host.style, {
      position: "fixed",
      inset: "0",
      zIndex: "2147483000",
      pointerEvents: "none",
      overflow: "visible",
    });
    let ancestor: Node | null = element.parentNode;
    let dialog: HTMLDialogElement | undefined;
    while (ancestor) {
      if (ancestor instanceof HTMLDialogElement && ancestor.open) {
        dialog = ancestor;
        break;
      }
      ancestor = ancestor instanceof ShadowRoot ? ancestor.host : ancestor.parentNode;
    }
    (dialog ?? document.body).append(host);
    const shadow = host.attachShadow({ mode: "open" });
    for (const sheet of [
      ...Array.from(origin.styleSheets ?? []),
      ...Array.from(origin.adoptedStyleSheets ?? []),
    ]) {
      try {
        const style = document.createElement("style");
        style.textContent = Array.from(sheet.cssRules, (rule) => rule.cssText).join("\n");
        shadow.append(style);
      } catch {
        /* 无跨域样式读取。 */
      }
    }
    injectStyles(shadow);
    const ancestors: Element[] = [];
    let parent: Node | null = element.parentNode;
    while (parent instanceof Element) {
      ancestors.unshift(parent);
      parent = parent.parentNode;
    }
    let container: Node = shadow;
    for (const parentElement of ancestors) {
      const wrapper = document.createElement("div");
      wrapper.className = parentElement.className;
      wrapper.style.display = "contents";
      container.appendChild(wrapper);
      container = wrapper;
    }
    const pointerEvents = element.style.pointerEvents;
    element.style.pointerEvents = "auto";
    container.appendChild(element);
    const onToggle = (event: Event) => {
      if ((event as Event & { newState?: string }).newState === "closed") restore(element);
    };
    let lifecycle: MutationObserver | undefined;
    const restorePortal = () => {
      lifecycle?.disconnect();
      element.removeEventListener("toggle", onToggle);
      if (placeholder.parentNode) placeholder.replaceWith(element);
      element.style.pointerEvents = pointerEvents;
      host.remove();
      portals.delete(element);
    };
    portals.set(element, { owner, host, restore: restorePortal });
    lifecycle = new MutationObserver((records) => {
      const removed = !shadow.contains(element);
      const hidden =
        records.some((record) => record.target === element && record.type === "attributes") &&
        (element.getAttribute("aria-hidden") === "true" || element.hidden);
      if (!removed && !hidden) return;
      // Material Menu 关闭时直接移除节点，不一定调用 hidePopover 或派发 toggle。
      // 先恢复已移除的节点，令 polyfill 能清理文档 top-layer 集合。
      if (removed) restorePortal();
      if (element.matches(":popover-open")) originalHide.call(element);
      restorePortal();
    });
    lifecycle.observe(shadow, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["aria-hidden", "hidden"],
    });
    sync();
    element.addEventListener("toggle", onToggle);
  }
  const patchedShow: typeof originalShow = function (this: HTMLElement, ...args) {
    move(this);
    try {
      const result = originalShow.apply(this, args);
      if (!this.matches(":popover-open")) restore(this);
      return result;
    } catch (error) {
      restore(this);
      throw error;
    }
  };
  HTMLElement.prototype.showPopover = patchedShow;
  const patchedHide: typeof originalHide = function (this: HTMLElement, ...args) {
    try {
      return originalHide.apply(this, args);
    } finally {
      if (!this.matches(":popover-open")) restore(this);
    }
  };
  const patchedToggle: typeof originalToggle = function (this: HTMLElement, ...args) {
    if (!this.matches(":popover-open")) move(this);
    try {
      const result = Reflect.apply(originalToggle, this, args) as boolean;
      if (!this.matches(":popover-open")) restore(this);
      return result;
    } catch (error) {
      restore(this);
      throw error;
    }
  };
  HTMLElement.prototype.hidePopover = patchedHide;
  HTMLElement.prototype.togglePopover = patchedToggle;
  const sync = () => {
    for (const [element, portal] of portals) {
      if (!portal.owner.isConnected) {
        if (element.matches(":popover-open")) originalHide.call(element);
        portal.restore();
        continue;
      }
      const computed = getComputedStyle(portal.owner);
      for (const property of Array.from(computed)) {
        if (property.startsWith("--mtrl-") || property.startsWith("--szu-"))
          portal.host.style.setProperty(property, computed.getPropertyValue(property));
      }
    }
  };
  const observer = new MutationObserver(sync);
  observer.observe(document.documentElement, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["data-theme-mode", "data-theme", "class"],
  });
  document.addEventListener("szu-theme-changed", sync);
  return () => {
    if (HTMLElement.prototype.showPopover === patchedShow)
      HTMLElement.prototype.showPopover = originalShow;
    if (HTMLElement.prototype.hidePopover === patchedHide)
      HTMLElement.prototype.hidePopover = originalHide;
    if (HTMLElement.prototype.togglePopover === patchedToggle)
      HTMLElement.prototype.togglePopover = originalToggle;
    observer.disconnect();
    document.removeEventListener("szu-theme-changed", sync);
    for (const portal of portals.values()) portal.restore();
  };
}
