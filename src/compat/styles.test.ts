// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vite-plus/test";
import { installStyleCompatibility, synchronizeHas } from "./styles";
import type { HasRule } from "./contracts";

afterEach(() => {
  document.body.innerHTML = "";
});
describe("scoped :has fallback", () => {
  it("retains a shared condition on every candidate, including document ancestors", () => {
    document.body.innerHTML = '<div class="app-frame"><main data-fullscreen></main></div>';
    const rules = [":root", "body", ".app-frame"].map((candidate) => ({
      attribute: "data-szu-has-fullscreen",
      candidate,
      conditions: ["[data-fullscreen]"],
    }));
    synchronizeHas(document, rules);
    for (const selector of [":root", "body", ".app-frame"])
      expect(document.querySelector(selector)!.hasAttribute("data-szu-has-fullscreen")).toBe(true);
    document.querySelector("main")!.removeAttribute("data-fullscreen");
    synchronizeHas(document, rules);
    expect(document.querySelectorAll("[data-szu-has-fullscreen]")).toHaveLength(0);
  });
  it("handles children, siblings, negation and state removal", () => {
    document.body.innerHTML =
      '<div class="control"><input disabled><span class="label"></span></div><p></p>';
    const rules: HasRule[] = [
      {
        attribute: "data-szu-has-disabled",
        candidate: ".control",
        conditions: ["> input:disabled"],
      },
      { attribute: "data-szu-has-next", candidate: ".control", conditions: ["+ p"] },
    ];
    synchronizeHas(document, rules);
    const control = document.querySelector(".control")!;
    expect(control.hasAttribute("data-szu-has-disabled")).toBe(true);
    expect(control.hasAttribute("data-szu-has-next")).toBe(true);
    document.querySelector("input")!.disabled = false;
    document.querySelector("p")!.remove();
    synchronizeHas(document, rules);
    expect(control.hasAttribute("data-szu-has-disabled")).toBe(false);
    expect(control.hasAttribute("data-szu-has-next")).toBe(false);
  });
  it("never crosses a shadow boundary and disconnects removed roots", () => {
    const controller = installStyleCompatibility(document, {
      has: true,
      colorMix: false,
      containers: false,
      viewport: false,
    });
    try {
      const host = document.createElement("div");
      document.body.append(host);
      const root = host.attachShadow({ mode: "open" });
      root.innerHTML = '<div class="control"><span class="label"></span></div>';
      const rules = [
        { attribute: "data-szu-has-label", candidate: ".control", conditions: [".label"] },
      ];
      synchronizeHas(document, rules);
      expect(root.querySelector(".control")!.hasAttribute("data-szu-has-label")).toBe(false);
      synchronizeHas(root, rules);
      expect(root.querySelector(".control")!.hasAttribute("data-szu-has-label")).toBe(true);
      controller.flush();
      expect(controller.rootCount()).toBe(2);
      host.remove();
      controller.flush();
      expect(controller.rootCount()).toBe(1);
    } finally {
      controller.dispose();
    }
  });
});
