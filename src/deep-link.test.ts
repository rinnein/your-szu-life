/** @vitest-environment jsdom */
import { beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { parseDeepLinkUrl, setupDeepLinkListener } from "./deep-link";

const startup = vi.hoisted(() => ({
  eventUrls: [] as string[],
  currentUrls: null as string[] | null,
  currentRejects: false,
}));

vi.mock("@tauri-apps/plugin-deep-link", () => ({
  onOpenUrl: vi.fn((callback: (urls: string[]) => void) => {
    if (startup.eventUrls.length > 0) callback([...startup.eventUrls]);
    return Promise.resolve(vi.fn());
  }),
  getCurrent: vi.fn(() =>
    startup.currentRejects
      ? Promise.reject(new Error("deep link state unavailable"))
      : Promise.resolve(startup.currentUrls),
  ),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, callback: (e: { payload: { route: string } }) => void) => {
    if (event === "navigate-page") {
      callback({ payload: { route: "/uooc" } });
    }
    return Promise.resolve(vi.fn());
  }),
}));

function routesOf(navigate: ReturnType<typeof vi.fn>) {
  return navigate.mock.calls.map(([route]) => route);
}

describe("deep-link handling", () => {
  beforeEach(() => {
    startup.eventUrls = [];
    startup.currentUrls = null;
    startup.currentRejects = false;
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  });

  it("correctly parses your-szu-life:// schemes", () => {
    expect(parseDeepLinkUrl("your-szu-life://page/electricity")).toBe("/electricity");
    expect(parseDeepLinkUrl("your-szu-life://page/uooc/courses")).toBe("/uooc/courses");
    expect(parseDeepLinkUrl("your-szu-life://workbench")).toBe("/workbench");
    expect(parseDeepLinkUrl("https://szu.edu.cn")).toBeNull();
  });

  it("sets up listeners and receives navigation calls", async () => {
    startup.eventUrls = ["your-szu-life://page/electricity"];
    const navigate = vi.fn();
    const cleanup = await setupDeepLinkListener(navigate);

    expect(routesOf(navigate)).toEqual(["/electricity", "/uooc"]);
    cleanup();
  });

  it("replays the launch URL when its event was emitted before the listener existed", async () => {
    // 第二实例在首实例 webview 就绪前转发链接时，事件会丢失，只剩插件缓存的 current。
    startup.currentUrls = ["your-szu-life://page/electricity"];
    const navigate = vi.fn();
    const cleanup = await setupDeepLinkListener(navigate);

    expect(routesOf(navigate)).toEqual(["/electricity", "/uooc"]);
    cleanup();
  });

  it("navigates once when the launch URL also arrives as a runtime event", async () => {
    startup.eventUrls = ["your-szu-life://page/electricity"];
    startup.currentUrls = ["your-szu-life://page/electricity"];
    const navigate = vi.fn();
    const cleanup = await setupDeepLinkListener(navigate);

    expect(routesOf(navigate).filter((route) => route === "/electricity")).toHaveLength(1);
    cleanup();
  });

  it("keeps listening when the startup URLs cannot be read", async () => {
    startup.currentRejects = true;
    startup.eventUrls = ["your-szu-life://page/electricity"];
    const navigate = vi.fn();
    const cleanup = await setupDeepLinkListener(navigate);

    expect(routesOf(navigate)).toEqual(["/electricity", "/uooc"]);
    cleanup();
  });
});
