/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { StrictMode, type ReactNode } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { AppChrome } from "./AppChrome";
import { PageSurface } from "./PageSurface";

vi.mock("@tanstack/react-router", () => ({
  useRouterState: () => "/",
}));

vi.mock("../logical-navigation", () => ({
  useLogicalNavigation: () => ({ push: vi.fn(), replace: vi.fn() }),
}));

vi.mock("./native-chrome", () => ({ useNativeChromeColors: () => {} }));
vi.mock("../motion/PageTransition", () => ({
  PageTransition: ({ children }: { children: ReactNode }) => <>{children}</>,
}));

function navigation() {
  return document.querySelector("m-navigation-bar, m-navigation-rail");
}

function layout(children: ReactNode) {
  return (
    <StrictMode>
      <AppChrome>{children}</AppChrome>
    </StrictMode>
  );
}

beforeEach(() => {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: false,
    media: query,
    addEventListener() {},
    removeEventListener() {},
  }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("AppChrome fullscreen pages", () => {
  it.each([false, true])("removes navigation for fullscreen pages (wide=%s)", (wide) => {
    vi.stubGlobal("matchMedia", (query: string) => ({
      matches: wide && query.includes("min-width: 900px"),
      media: query,
      addEventListener() {},
      removeEventListener() {},
    }));
    const { rerender } = render(layout(<PageSurface>普通页面</PageSurface>));
    expect(navigation()?.localName).toBe(wide ? "m-navigation-rail" : "m-navigation-bar");

    rerender(layout(<PageSurface fullscreen>初始化设置</PageSurface>));
    // DOM 中没有导航，所以即使 CSS :has() 完全不可用，也不会遮挡底部操作。
    expect(navigation()).toBeNull();
    expect(document.querySelector(".app-frame")).toHaveAttribute("data-fullscreen");
    expect(screen.getByText("初始化设置")).toBeInTheDocument();

    rerender(layout(<PageSurface>普通页面</PageSurface>));
    expect(navigation()?.localName).toBe(wide ? "m-navigation-rail" : "m-navigation-bar");
    expect(document.querySelector(".app-frame")).not.toHaveAttribute("data-fullscreen");
  });

  it("treats centered authentication pages as fullscreen", () => {
    render(layout(<PageSurface centered>正在认证</PageSurface>));
    expect(navigation()).toBeNull();
  });

  it("keeps navigation hidden until the last fullscreen page unmounts", () => {
    const pages = (first: boolean, second: boolean) =>
      layout(
        <>
          {first && (
            <PageSurface key="first" fullscreen>
              第一个全屏页面
            </PageSurface>
          )}
          {second && (
            <PageSurface key="second" centered>
              第二个全屏页面
            </PageSurface>
          )}
          <PageSurface>普通页面</PageSurface>
        </>,
      );
    const { rerender } = render(pages(true, true));
    expect(navigation()).toBeNull();

    rerender(pages(false, true));
    expect(navigation()).toBeNull();

    rerender(pages(false, false));
    expect(navigation()).toBeInTheDocument();
  });
});
