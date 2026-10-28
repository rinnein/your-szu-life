/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { afterEach, expect, it, vi } from "vite-plus/test";
import {
  pushNativeChromeColors,
  readNativeChromeColors,
  resetNativeChromeColors,
  rgbToHex,
} from "./native-chrome";

afterEach(() => {
  resetNativeChromeColors();
  delete window.SzuApplicationBack;
  vi.restoreAllMocks();
  document.body.replaceChildren();
});

it("converts the rgb() values getComputedStyle returns into hex", () => {
  expect(rgbToHex("rgb(252, 248, 255)")).toBe("#fcf8ff");
  expect(rgbToHex("rgb(1,2,3)")).toBe("#010203");
  expect(rgbToHex(" rgba(21, 18, 24, 0.6) ")).toBe("#151218");
});

it("rejects values that are not an opaque system-bar colour", () => {
  for (const value of ["", "transparent", "rgba(0, 0, 0, 0)", "#fcf8ff", "rgb(300,0,0)", null]) {
    expect(rgbToHex(value)).toBeUndefined();
  }
});

const backgrounds = new WeakMap<Element, string>();
const tokens = new Map<string, string>();

function installComputedStyles() {
  vi.spyOn(window, "getComputedStyle").mockImplementation(
    (element: Element) =>
      ({
        backgroundColor: backgrounds.get(element) ?? "",
        getPropertyValue: (name: string) => tokens.get(name) ?? "",
      }) as unknown as CSSStyleDeclaration,
  );
}

/**
 * 造一个带 shadow root 的顶栏宿主：颜色读自它内部的外壳元素（material 的结构）。
 * 只对 `.mtrl-top-app-bar` 命中，顺带守住「在 shadow root 内不能再用宿主标签选择器」。
 */
function mountTopAppBar(backgroundColor: string) {
  const shell = document.createElement("header");
  shell.className = "mtrl-top-app-bar";
  backgrounds.set(shell, backgroundColor);
  const host = document.createElement("m-top-app-bar");
  Object.defineProperty(host, "shadowRoot", {
    value: {
      querySelector: (selector: string) => (selector === ".mtrl-top-app-bar" ? shell : null),
    },
  });
  document.body.append(host);
  return shell;
}

function mountNavigationBar(backgroundColor: string) {
  const nav = document.createElement("m-navigation-bar");
  nav.className = "app-nav-bar";
  backgrounds.set(nav, backgroundColor);
  document.body.append(nav);
  return nav;
}

it("takes the status bar colour from the top app bar and the gesture bar from the bottom navigation bar", () => {
  mountTopAppBar("rgb(252, 248, 255)");
  mountNavigationBar("rgb(246, 242, 250)");
  installComputedStyles();

  expect(readNativeChromeColors()).toEqual({ statusBar: "#fcf8ff", navigationBar: "#f6f2fa" });
});

it("follows the top app bar when its colour changes", () => {
  // 顶栏滚到「已滚动」状态后底色从 surface 变成 surface-container，状态栏要跟着变。
  const shell = mountTopAppBar("rgb(252, 248, 255)");
  installComputedStyles();
  expect(readNativeChromeColors().statusBar).toBe("#fcf8ff");

  backgrounds.set(shell, "rgb(19, 19, 24)");
  expect(readNativeChromeColors().statusBar).toBe("#131318");
});

it("falls back to the theme roles when there is no such control", () => {
  // 认证流程没有顶栏，宽屏也没有底部导航栏，此时按主题角色取色。
  tokens.set("--mtrl-sys-color-surface", "#fcf8ff");
  tokens.set("--mtrl-sys-color-surface-container", "#f6f2fa");
  installComputedStyles();

  expect(readNativeChromeColors()).toEqual({ statusBar: "#fcf8ff", navigationBar: "#f6f2fa" });
});

it("posts each system bar colour once and only when it changes", () => {
  const postMessage = vi.fn();
  window.SzuApplicationBack = { postMessage, onmessage: null };

  pushNativeChromeColors({ statusBar: "#fcf8ff", navigationBar: "#f6f2fa" });
  expect(postMessage.mock.calls.map(([message]) => message)).toEqual([
    "chrome-status-bar:#fcf8ff",
    "chrome-nav-bar:#f6f2fa",
  ]);

  // 同一个值再推一次不发消息：滚动时每一帧都会重读颜色。
  postMessage.mockClear();
  pushNativeChromeColors({ statusBar: "#fcf8ff", navigationBar: "#f6f2fa" });
  expect(postMessage).not.toHaveBeenCalled();

  // 顶栏变色只发状态栏；小白条固定不动，不重复发。
  pushNativeChromeColors({ statusBar: "#131318", navigationBar: "#f6f2fa" });
  expect(postMessage.mock.calls.map(([message]) => message)).toEqual(["chrome-status-bar:#131318"]);
});

it("stays silent without the Android bridge", () => {
  const postMessage = vi.fn();
  pushNativeChromeColors({ statusBar: "#fcf8ff", navigationBar: "#f6f2fa" });
  expect(postMessage).not.toHaveBeenCalled();
});
