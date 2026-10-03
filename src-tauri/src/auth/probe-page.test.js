/** @vitest-environment node */
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { JSDOM } from "jsdom";
import authProbePage from "./probe-page.js";

const pages = [];
function page(fetch) {
  const dom = new JSDOM("", {
    url: "https://authserver.szu.edu.cn/personalInfo/personCenter/index.html",
    runScripts: "outside-only",
  });
  pages.push(dom);
  Object.assign(dom.window, { fetch, setTimeout, clearTimeout, Date });
  const evaluate = (operation = "start", generation = "1", budgetMs = 90000) =>
    dom.window.eval(
      `(${authProbePage.toString()})(${JSON.stringify({ operation, generation, budgetMs })})`,
    );
  return { window: dom.window, evaluate };
}
const response = (status) => ({
  status,
  ok: status === 200,
  url: "https://authserver.szu.edu.cn/personalInfo/common/getUserConf",
  text: async () => "{}",
});
beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  for (const dom of pages.splice(0)) {
    dom.window.__SZU_AUTH_PROBE_STATE__?.dispose();
    dom.window.close();
  }
  vi.useRealTimers();
});

describe("auth identity probe on slow networks", () => {
  it("does not restart a pending request during repeated native polling or injection", async () => {
    let resolve;
    const fetch = vi.fn(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    );
    const p = page(fetch);
    p.evaluate();
    for (let i = 0; i < 80; i++) {
      p.evaluate();
      expect(p.evaluate("poll")).toBeNull();
      await vi.advanceTimersByTimeAsync(250);
    }
    expect(fetch).toHaveBeenCalledTimes(1);
    resolve(response(200));
    await vi.advanceTimersByTimeAsync(0);
    expect(p.evaluate("poll")).toMatchObject({ ok: true, status: 200 });
    expect(p.evaluate("poll")).toBeNull();
    await vi.advanceTimersByTimeAsync(5000);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("backs off transient errors but never retries a definitive rejection", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(response(503))
      .mockResolvedValueOnce(response(429))
      .mockResolvedValue(response(401));
    const p = page(fetch);
    p.evaluate();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(999);
    expect(fetch).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(fetch).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(2000);
    expect(fetch).toHaveBeenCalledTimes(3);
    expect(p.evaluate("poll")).toMatchObject({ ok: false, status: 401 });
    await vi.advanceTimersByTimeAsync(90000);
    expect(fetch).toHaveBeenCalledTimes(3);
  });

  it("aborts a hung request and stops retrying at the native deadline", async () => {
    const fetch = vi.fn(
      (_url, { signal }) =>
        new Promise((_resolve, reject) => {
          signal.addEventListener("abort", () => reject(new Error("aborted")));
        }),
    );
    const p = page(fetch);
    p.evaluate("start", "1", 35000);
    await vi.advanceTimersByTimeAsync(30000);
    expect(fetch).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1000);
    expect(fetch).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(5000);
    expect(p.evaluate("poll")).toMatchObject({ ok: false, reason: "timeout" });
    await vi.advanceTimersByTimeAsync(90000);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("discards cancelled requests and ignores an older queued generation", async () => {
    const resolvers = [];
    const fetch = vi.fn(() => new Promise((resolve) => resolvers.push(resolve)));
    const p = page(fetch);
    p.evaluate();
    p.evaluate("start", "2");
    p.evaluate("start", "1");
    expect(fetch).toHaveBeenCalledTimes(2);
    resolvers[0](response(200));
    await vi.advanceTimersByTimeAsync(0);
    expect(p.evaluate("poll", "2")).toBeNull();
    p.window.dispatchEvent(new p.window.Event("pagehide"));
    resolvers[1](response(200));
    await vi.advanceTimersByTimeAsync(0);
    expect(p.evaluate("poll", "2")).toBeNull();
  });
});
