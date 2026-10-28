import { describe, expect, it } from "vite-plus/test";
import { activeDestination, DESTINATIONS } from "./destinations";

describe("navigation destinations", () => {
  it("exposes exactly the four shared rail/bar entries", () => {
    expect(DESTINATIONS.map((item) => [item.value, item.label, item.to])).toEqual([
      ["home", "首页", "/"],
      ["study", "学习", "/study"],
      ["life", "生活", "/life"],
      ["settings", "设置", "/settings"],
    ]);
  });

  it("resolves the owning destination from the first path segment", () => {
    expect(activeDestination("/")).toBe("home");
    expect(activeDestination("/study")).toBe("study");
    expect(activeDestination("/study/uooc")).toBe("study");
    expect(activeDestination("/study/enrollment/login")).toBe("study");
    expect(activeDestination("/study/oj")).toBe("study");
    expect(activeDestination("/life")).toBe("life");
    expect(activeDestination("/life/campus-network")).toBe("life");
    expect(activeDestination("/life/campus-payments/electricity")).toBe("life");
    expect(activeDestination("/settings")).toBe("settings");
  });

  it("falls back to the home destination for unknown paths", () => {
    expect(activeDestination("/unknown")).toBe("home");
    expect(activeDestination("")).toBe("home");
  });
});
