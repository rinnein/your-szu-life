/** @vitest-environment jsdom */
import { describe, expect, it, vi } from "vite-plus/test";
import { getArch, getOsType, getOsVersion, getPlatform, isAndroidPlatform } from "./os-info";

vi.mock("@tauri-apps/plugin-os", () => ({
  platform: vi.fn().mockResolvedValue("linux"),
  type: vi.fn().mockResolvedValue("linux"),
  version: vi.fn().mockResolvedValue("6.8.0"),
  arch: vi.fn().mockResolvedValue("x86_64"),
}));

describe("os-info plugin integration", () => {
  it("detects platform when running in Tauri", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
    expect(await getPlatform()).toBe("linux");
    expect(await getOsType()).toBe("linux");
    expect(await getOsVersion()).toBe("6.8.0");
    expect(await getArch()).toBe("x86_64");
    expect(await isAndroidPlatform()).toBe(false);
  });

  it("falls back to userAgent outside Tauri", async () => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    expect(typeof (await getPlatform())).toBe("string");
  });
});
