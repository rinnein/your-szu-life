/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { routeTree } from "../routeTree.gen";
import { initialLogicalEntries } from "../logical-navigation";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

function renderAt(path: string) {
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({
      initialEntries: initialLogicalEntries(`https://tauri.localhost${path}`),
    }),
  });
  render(<RouterProvider router={router} />);
  return router;
}

/**
 * material 的导航项把 role 交给 ElementInternals，jsdom 拿不到可访问角色，
 * 因此直接断言声明元素上的 label。
 */
const NAV_LABELS = ["首页", "学习", "生活", "设置"];

function labelsOf(tag: string) {
  return Array.from(document.querySelectorAll(tag)).map((item) => item.getAttribute("label"));
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listen).mockResolvedValue(() => undefined);
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_auth_state")
      return { Authenticated: { studentId: "1234567890", name: "测试同学" } };
    if (command === "get_account_profile")
      return {
        studentId: "1234567890",
        campusCardNumber: "123456",
        hasPassword: true,
        name: "测试同学",
      };
    if (command === "get_user_data") return { studentId: "1234567890", name: "测试同学" };
    return undefined;
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("AppNavigation", () => {
  it("shows the same four destinations in the narrow navigation bar", async () => {
    renderAt("/");
    await screen.findByRole("heading", { name: "首页" });

    expect(labelsOf("m-navigation-bar-item")).toEqual(NAV_LABELS);
    expect(document.querySelectorAll("m-navigation-rail-item")).toHaveLength(0);
  });

  it("shows the same four destinations in the wide navigation rail", async () => {
    vi.stubGlobal(
      "matchMedia",
      vi.fn().mockImplementation((query: string) => ({
        media: query,
        matches: query.includes("min-width: 900px"),
        addEventListener() {},
        removeEventListener() {},
        addListener() {},
        removeListener() {},
      })),
    );
    renderAt("/");
    await screen.findByRole("heading", { name: "首页" });

    expect(labelsOf("m-navigation-rail-item")).toEqual(NAV_LABELS);
    expect(document.querySelectorAll("m-navigation-bar-item")).toHaveLength(0);
  });

  it("keeps the same four destinations on a nested child page", async () => {
    renderAt("/study/uooc");
    await screen.findByRole("heading", { name: "UOOC刷课" });

    expect(labelsOf("m-navigation-bar-item")).toEqual(NAV_LABELS);
  });

  it("sets --app-nav-bar-height on document root when narrow navigation bar has measured height", async () => {
    const originalGetBoundingClientRect = HTMLElement.prototype.getBoundingClientRect;
    HTMLElement.prototype.getBoundingClientRect = function () {
      if (this.classList.contains("app-nav-bar")) {
        return {
          height: 65,
          width: 390,
          top: 779,
          bottom: 844,
          left: 0,
          right: 390,
          x: 0,
          y: 779,
          toJSON: () => {},
        };
      }
      return originalGetBoundingClientRect.call(this);
    };

    try {
      renderAt("/");
      await screen.findByRole("heading", { name: "首页" });
      expect(document.documentElement.style.getPropertyValue("--app-nav-bar-height")).toBe("65px");
    } finally {
      HTMLElement.prototype.getBoundingClientRect = originalGetBoundingClientRect;
      document.documentElement.style.removeProperty("--app-nav-bar-height");
    }
  });

  it("clears --app-nav-bar-height on wide screen rail navigation", async () => {
    document.documentElement.style.setProperty("--app-nav-bar-height", "65px");
    vi.stubGlobal(
      "matchMedia",
      vi.fn().mockImplementation((query: string) => ({
        media: query,
        matches: query.includes("min-width: 900px"),
        addEventListener() {},
        removeEventListener() {},
        addListener() {},
        removeListener() {},
      })),
    );
    renderAt("/");
    await screen.findByRole("heading", { name: "首页" });
    expect(document.documentElement.style.getPropertyValue("--app-nav-bar-height")).toBe("");
  });
});
