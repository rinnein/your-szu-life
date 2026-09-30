import { describe, expect, it } from "vite-plus/test";
import {
  consumeAuthReturn,
  createReloadableLogicalHistory,
  LOGICAL_HISTORY_KEY,
} from "./logical-history";

function storage() {
  const values = new Map<string, string>();
  return {
    getItem: (key: string) => values.get(key) ?? null,
    removeItem: (key: string) => {
      values.delete(key);
    },
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
  };
}

describe("logical navigation across WebView reload", () => {
  it("restores the logical parents once after returning from official authentication", () => {
    const store = storage();
    const href = "https://tauri.localhost/campus-payments/network?tab=current#details";
    const history = createReloadableLogicalHistory({
      href: "https://tauri.localhost/",
      reload: false,
      storage: store,
    });
    history.push("/settings");
    history.push("/campus-payments/network?tab=current#details");
    store.setItem("szu.auth-return.v1", href);
    const restored = createReloadableLogicalHistory({
      href,
      reload: consumeAuthReturn(store, href),
      storage: store,
    });
    expect(restored.length).toBe(3);
    restored.back();
    expect(restored.location.pathname).toBe("/settings");
    expect(consumeAuthReturn(store, href)).toBe(false);
    store.setItem("szu.auth-return.v1", href);
    expect(consumeAuthReturn(store, "https://tauri.localhost/about")).toBe(false);
  });
  it("restores nested parents and tab replacements, then returns to the same parent", () => {
    const store = storage();
    const history = createReloadableLogicalHistory({
      href: "https://tauri.localhost/",
      reload: false,
      storage: store,
    });
    history.push("/settings");
    history.push("/campus-payments/card");
    history.replace("/campus-payments/network");
    const reloaded = createReloadableLogicalHistory({
      href: "https://tauri.localhost/campus-payments/network",
      reload: true,
      storage: store,
    });
    expect(reloaded.length).toBe(3);
    expect(reloaded.location.pathname).toBe("/campus-payments/network");
    reloaded.back();
    expect(reloaded.location.pathname).toBe("/settings");
    reloaded.back();
    expect(reloaded.canGoBack()).toBe(false);
  });

  it("restores root cursor and forward entries, and truncates them on a new push", () => {
    const store = storage();
    const history = createReloadableLogicalHistory({
      href: "https://tauri.localhost/",
      reload: false,
      storage: store,
    });
    history.push("/settings");
    history.back();
    const restored = createReloadableLogicalHistory({
      href: "https://tauri.localhost/",
      reload: true,
      storage: store,
    });
    expect(restored.location.pathname).toBe("/");
    expect(restored.canGoBack()).toBe(false);
    restored.forward();
    expect(restored.location.pathname).toBe("/settings");
    restored.back();
    restored.push("/about");
    expect(JSON.parse(store.getItem(LOGICAL_HISTORY_KEY)!).entries).toEqual(["/", "/about"]);
  });

  it("keeps a child window as its own root", () => {
    const store = storage();
    const href = "https://tauri.localhost/campus-payments/network?__szu_window=child";
    createReloadableLogicalHistory({ href, reload: false, storage: store });
    const restored = createReloadableLogicalHistory({ href, reload: true, storage: store });
    expect(restored.length).toBe(1);
    expect(restored.canGoBack()).toBe(false);
  });

  it.each([
    "bad JSON",
    "null",
    JSON.stringify({
      version: 1,
      child: false,
      entries: ["/", "https://evil.example", "/settings"],
      index: 2,
    }),
    JSON.stringify({
      version: 1,
      child: false,
      entries: ["/", "/\\evil.example", "/settings"],
      index: 2,
    }),
    JSON.stringify({ version: 1, child: false, entries: ["/", "/about"], index: 1 }),
    JSON.stringify({ version: 1, child: true, entries: ["/settings"], index: 0 }),
    JSON.stringify({ version: 1, child: false, entries: ["/", "/settings"], index: -1 }),
  ])("falls back to deep-link context for an invalid snapshot: %s", (raw) => {
    const store = storage();
    store.setItem(LOGICAL_HISTORY_KEY, raw);
    const restored = createReloadableLogicalHistory({
      href: "https://tauri.localhost/settings",
      reload: true,
      storage: store,
    });
    expect(restored.length).toBe(2);
    restored.back();
    expect(restored.location.pathname).toBe("/");
  });

  it("ignores old snapshots on a fresh navigation and tolerates blocked storage", () => {
    const store = storage();
    store.setItem(
      LOGICAL_HISTORY_KEY,
      JSON.stringify({ version: 1, child: false, entries: ["/", "/about", "/settings"], index: 2 }),
    );
    expect(
      createReloadableLogicalHistory({
        href: "https://tauri.localhost/settings",
        reload: false,
        storage: store,
      }).length,
    ).toBe(2);
    const blocked = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };
    const history = createReloadableLogicalHistory({
      href: "https://tauri.localhost/settings",
      reload: true,
      storage: blocked,
    });
    expect(() => history.push("/about")).not.toThrow();
    history.back();
    expect(history.location.pathname).toBe("/settings");
  });
});
