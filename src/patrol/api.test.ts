import { describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import {
  mapWithConcurrency,
  pauseDownload,
  removeDownload,
  resumeDownload,
  toProxyUrl,
  weekdayOf,
} from "./api";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

describe("toProxyUrl", () => {
  it("把媒体地址编码为 jxxk-stream 代理地址", () => {
    const target = "https://jxxk.szu.edu.cn/vv/abc/x.mp4/index.m3u8?t=1&sign=a+b/c";
    const proxied = toProxyUrl(target);
    expect(proxied.startsWith("jxxk-stream://stream/")).toBe(true);
    const encoded = proxied.slice("jxxk-stream://stream/".length);
    // base64url 字符集不应包含标准 base64 的 + / =
    expect(encoded).not.toMatch(/[+/=]/);
    const binary = atob(encoded.replace(/-/g, "+").replace(/_/g, "/"));
    const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
    expect(new TextDecoder().decode(bytes)).toBe(target);
  });
});

describe("weekdayOf", () => {
  it("解析平台时间串为星期几（1=周一 … 7=周日）", () => {
    expect(weekdayOf("2026-09-04 10:15:00")).toBe(5);
    expect(weekdayOf("2026-09-07 08:00:00")).toBe(1);
    // 2026-11-01 是星期日
    expect(weekdayOf("2026-11-01 09:00:00")).toBe(7);
  });

  it("空值与无效时间返回 null", () => {
    expect(weekdayOf(null)).toBeNull();
    expect(weekdayOf("")).toBeNull();
    expect(weekdayOf("不是时间")).toBeNull();
  });
});

describe("mapWithConcurrency", () => {
  it("按并发限制执行并保持结果顺序", async () => {
    let running = 0;
    let peak = 0;
    const results = await mapWithConcurrency([1, 2, 3, 4, 5, 6], 2, async (n) => {
      running += 1;
      peak = Math.max(peak, running);
      await new Promise((resolve) => setTimeout(resolve, 5));
      running -= 1;
      return n * 10;
    });
    expect(results).toEqual([10, 20, 30, 40, 50, 60]);
    expect(peak).toBeLessThanOrEqual(2);
  });

  it("空数组直接返回空结果", async () => {
    expect(await mapWithConcurrency([], 4, async (n) => n)).toEqual([]);
  });
});

describe("download management api", () => {
  it("pauseDownload 传参 taskId 调用 jxxk_download_pause", async () => {
    await pauseDownload("task-123");
    expect(invoke).toHaveBeenCalledWith("jxxk_download_pause", { taskId: "task-123" });
  });

  it("resumeDownload 传参 taskId 调用 jxxk_download_resume", async () => {
    await resumeDownload("task-123");
    expect(invoke).toHaveBeenCalledWith("jxxk_download_resume", { taskId: "task-123" });
  });

  it("removeDownload 传参 taskId 与 deleteFile 调用 jxxk_download_remove", async () => {
    await removeDownload("task-123", true);
    expect(invoke).toHaveBeenCalledWith("jxxk_download_remove", {
      taskId: "task-123",
      deleteFile: true,
    });

    await removeDownload("task-123", false);
    expect(invoke).toHaveBeenCalledWith("jxxk_download_remove", {
      taskId: "task-123",
      deleteFile: false,
    });
  });
});
