/** @vitest-environment jsdom */
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { ModuleActivityBridge, moduleForPath } from "./ModuleActivityBridge";

let pathname = "/";
vi.mock("@tanstack/react-router", () => ({
  useLocation: () => ({ pathname }),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));

describe("ModuleActivityBridge", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    pathname = "/";
  });

  afterEach(cleanup);

  it("maps feature routes to module names", () => {
    expect(moduleForPath("/study/oj")).toBe("oj");
    expect(moduleForPath("/study/oj?site=1")).toBe("oj");
    expect(moduleForPath("/study/uooc")).toBe("uooc");
    expect(moduleForPath("/study/patrol")).toBe("jxxk");
    expect(moduleForPath("/study/enrollment/tasks")).toBe("enrollment");
    expect(moduleForPath("/study/academic")).toBe("enrollment");
    expect(moduleForPath("/study/lms")).toBe("lms");
    expect(moduleForPath("/life/campus-network")).toBe("campus-network");
    expect(moduleForPath("/life/campus-payments/recharge")).toBe("electricity");
    expect(moduleForPath("/")).toBeUndefined();
  });

  it("reports the active module on mount and clears it on unmount", async () => {
    pathname = "/study/oj";
    const view = render(<ModuleActivityBridge />);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_active_module", {
        module: "oj",
        active: true,
      }),
    );

    view.unmount();
    expect(invoke).toHaveBeenCalledWith("set_active_module", {
      module: "oj",
      active: false,
    });
  });

  it("does not report anything outside feature routes", () => {
    pathname = "/";
    render(<ModuleActivityBridge />);
    expect(invoke).not.toHaveBeenCalledWith("set_active_module", expect.anything());
  });
});
