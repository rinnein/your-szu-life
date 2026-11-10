// @vitest-environment jsdom
import { expect, it } from "vite-plus/test";
import { installPopoverPortals } from "./popover";

it("restores and cleans a clipped overlay when Material removes it without toggle", async () => {
  const methods = ["showPopover", "hidePopover", "togglePopover"] as const;
  const descriptors = methods.map((name) =>
    Object.getOwnPropertyDescriptor(HTMLElement.prototype, name),
  );
  const matches = Element.prototype.matches;
  Element.prototype.matches = function (this: Element, selector: string) {
    return selector === ":popover-open"
      ? this.hasAttribute("data-test-open")
      : matches.call(this, selector);
  } as typeof matches;
  Object.defineProperties(HTMLElement.prototype, {
    showPopover: {
      configurable: true,
      writable: true,
      value(this: HTMLElement) {
        this.setAttribute("data-test-open", "");
      },
    },
    hidePopover: {
      configurable: true,
      writable: true,
      value(this: HTMLElement) {
        this.removeAttribute("data-test-open");
      },
    },
    togglePopover: {
      configurable: true,
      writable: true,
      value(this: HTMLElement) {
        return this.toggleAttribute("data-test-open");
      },
    },
  });
  const dispose = installPopoverPortals(() => {});
  const clipped = document.createElement("div");
  clipped.style.overflowX = "hidden";
  const owner = document.createElement("div");
  clipped.append(owner);
  document.body.append(clipped);
  const shadow = owner.attachShadow({ mode: "open" });
  const overlay = document.createElement("div");
  overlay.setAttribute("popover", "manual");
  shadow.append(overlay);
  try {
    expect(overlay.getRootNode() instanceof ShadowRoot).toBe(true);
    overlay.showPopover();
    expect(overlay.getAttribute("data-test-open")).toBe("");
    expect(overlay.getRootNode()).not.toBe(shadow);
    expect(document.querySelector("[data-szu-popover-portal]")).not.toBeNull();
    overlay.remove();
    await new Promise<void>((resolve) => queueMicrotask(resolve));
    expect(document.querySelector("[data-szu-popover-portal]")).toBeNull();
    expect(overlay.parentNode).toBe(shadow);
    expect(overlay.matches(":popover-open")).toBe(false);
  } finally {
    dispose();
    clipped.remove();
    Element.prototype.matches = matches;
    methods.forEach((name, i) => {
      if (descriptors[i]) Object.defineProperty(HTMLElement.prototype, name, descriptors[i]!);
      else Reflect.deleteProperty(HTMLElement.prototype, name);
    });
  }
});
