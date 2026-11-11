/** @vitest-environment node */
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { JSDOM } from "jsdom";
import localNotificationPage from "./local-page.js";

const pages = [];

function page(url, devUrl = null) {
  const dom = new JSDOM("", { url, runScripts: "outside-only" });
  pages.push(dom);
  const originalNotification = function BrowserNotification() {};
  dom.window.Notification = originalNotification;
  const initialize = vi.fn(() => {
    dom.window.Notification = function AppNotification() {};
  });
  dom.window.initializeNotification = initialize;
  dom.window.eval(
    `(${localNotificationPage.toString()})(${JSON.stringify(devUrl)}, window.initializeNotification)`,
  );
  return { window: dom.window, initialize, originalNotification };
}

afterEach(() => {
  for (const dom of pages.splice(0)) dom.window.close();
});

describe("notification initialization origin boundary", () => {
  it.each([
    "https://authserver.szu.edu.cn/",
    "https://authserver.szu.edu.cn/authserver/login",
    "https://authserver.szu.edu.cn/personalInfo/personalMobile/index.html",
    "https://net.szu.edu.cn/",
    "https://www.uooc.net.cn/",
    "about:blank",
    "file:///tmp/index.html",
    "tauri://external/",
    "https://tauri.localhost.evil.example/",
    "http://tauri.localhost:1420/",
    "http://user@tauri.localhost/",
    "http://localhost:1420/",
  ])("preserves the website Notification API without initializing on %s", (url) => {
    const p = page(url);
    expect(p.initialize).not.toHaveBeenCalled();
    expect(p.window.Notification).toBe(p.originalNotification);
  });

  it.each([
    "tauri://localhost/",
    "tauri://localhost/uooc?tab=tasks",
    "http://tauri.localhost/",
    "http://tauri.localhost/settings",
    "https://tauri.localhost/",
  ])("initializes the plugin on the packaged application URL %s", (url) => {
    const p = page(url);
    expect(p.initialize).toHaveBeenCalledTimes(1);
    expect(p.window.Notification).not.toBe(p.originalNotification);
  });

  it("allows the configured development origin, including Android LAN development", () => {
    for (const base of ["http://localhost:1420", "http://192.168.1.10:1420"]) {
      expect(page(`${base}/uooc?tab=tasks`, base).initialize).toHaveBeenCalledTimes(1);
    }
    const configured = "http://localhost:1420";
    for (const url of [
      "https://localhost:1420/",
      "http://localhost:1421/",
      "http://localhost.evil.example:1420/",
      "https://authserver.szu.edu.cn/",
    ]) {
      expect(page(url, configured).initialize).not.toHaveBeenCalled();
    }
  });
});
