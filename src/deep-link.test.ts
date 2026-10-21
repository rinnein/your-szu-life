/** @vitest-environment jsdom */
import { describe, expect, it, vi } from "vite-plus/test";
import { parseDeepLinkUrl, setupDeepLinkListener } from "./deep-link";

vi.mock("@tauri-apps/plugin-deep-link", () => ({
  onOpenUrl: vi.fn((callback: (urls: string[]) => void) => {
    callback(["your-szu-life://page/electricity"]);
    return Promise.resolve(vi.fn());
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, callback: (e: { payload: { route: string } }) => void) => {
    if (event === "navigate-page") {
      callback({ payload: { route: "/uooc" } });
    }
    return Promise.resolve(vi.fn());
  }),
}));

describe("deep-link handling", () => {
  it("correctly parses your-szu-life:// schemes", () => {
    expect(parseDeepLinkUrl("your-szu-life://page/electricity")).toBe("/electricity");
    expect(parseDeepLinkUrl("your-szu-life://page/uooc/courses")).toBe("/uooc/courses");
    expect(parseDeepLinkUrl("your-szu-life://workbench")).toBe("/workbench");
    expect(parseDeepLinkUrl("https://szu.edu.cn")).toBeNull();
  });

  it("sets up listeners and receives navigation calls", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
    const navigate = vi.fn();
    const cleanup = await setupDeepLinkListener(navigate);

    expect(navigate).toHaveBeenCalledWith("/electricity");
    expect(navigate).toHaveBeenCalledWith("/uooc");
    cleanup();
  });
});
