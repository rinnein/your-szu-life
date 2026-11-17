import { describe, expect, it } from "vite-plus/test";
import { pageTransitionKind } from "./page-transition-kind";

describe("pageTransitionKind", () => {
  it("plays the forward entrance on the first mount and on push", () => {
    expect(pageTransitionKind(null, null, "home")).toBe("forward");
    expect(pageTransitionKind("push", "study", "study")).toBe("forward");
  });

  it("plays the back entrance on back", () => {
    expect(pageTransitionKind("back", "study", "home")).toBe("back");
    // 回退到同一个部件同样是回退层级的入场。
    expect(pageTransitionKind("back", "study", "study")).toBe("back");
  });

  it("uses fade-through when a replace lands on another top-level destination", () => {
    expect(pageTransitionKind("replace", "home", "study")).toBe("fade-through");
    expect(pageTransitionKind("replace", "life", "settings")).toBe("fade-through");
  });

  it("stays still for in-page tab and filter replaces", () => {
    expect(pageTransitionKind("replace", "settings", "settings")).toBe("none");
    expect(pageTransitionKind("replace", "study", "study")).toBe("none");
    // 首次挂载之外的 replace 若拿不到上一部件，按同页处理，不误播整页动画。
    expect(pageTransitionKind("replace", null, "settings")).toBe("none");
  });
});
