/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { routeTree } from "../routeTree.gen";
import { initialLogicalEntries } from "../logical-navigation";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

function renderAt(path: string) {
  const history = createMemoryHistory({
    initialEntries: initialLogicalEntries(`https://tauri.localhost${path}`),
  });
  const router = createRouter({ routeTree, history });
  render(<RouterProvider router={router} />);
  return router;
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

afterEach(cleanup);

describe("TopAppBar", () => {
  it("hides the back button on the home page but keeps it on child pages", async () => {
    renderAt("/");
    expect(await screen.findByRole("heading", { name: "首页" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "返回上一页" })).not.toBeInTheDocument();
    cleanup();

    const router = renderAt("/study/uooc");
    expect(await screen.findByRole("button", { name: "返回上一页" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "返回上一页" }));
    // 嵌套后返回上一级是「学习」页。
    await vi.waitFor(() => expect(router.state.location.pathname).toBe("/study"));
    expect(router.history.canGoBack()).toBe(true);
  });

  it.each(["/study", "/life", "/settings"])(
    "leaves the back button off the top-level %s page",
    async (path) => {
      renderAt(path);

      expect(await screen.findByRole("button", { name: "打开命令面板搜索" })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "返回上一页" })).not.toBeInTheDocument();
    },
  );

  it("leaves the scroll state to material instead of pinning it", async () => {
    renderAt("/study");
    await screen.findByRole("button", { name: "打开命令面板搜索" });

    // 不传 no-scroll：material 按 window.scrollY 自行切换已滚动态，
    // 顶栏的吸顶与背景交给 CSS（.top-app-bar / ::part(top-app-bar)）。
    const bar = document.querySelector("m-top-app-bar");
    expect(bar).not.toBeNull();
    expect(bar!.hasAttribute("no-scroll")).toBe(false);
    expect(
      bar!.shadowRoot
        ?.querySelector(".mtrl-top-app-bar")
        ?.classList.contains("mtrl-top-app-bar--scrolled"),
    ).toBe(false);
  });

  it("opens the command palette from the search action on every page", async () => {
    renderAt("/study");

    const search = await screen.findByRole("button", { name: "打开命令面板搜索" });
    // 命令面板入口不再是首页的独立横条，只出现在顶栏。
    expect(screen.queryByRole("button", { name: /打开命令面板快捷搜索/ })).not.toBeInTheDocument();
    fireEvent.click(search);
    expect(await screen.findByRole("textbox", { name: "命令面板搜索框" })).toBeInTheDocument();
  });
});
