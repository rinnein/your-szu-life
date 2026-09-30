/** @vitest-environment jsdom */

import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { initialLogicalEntries } from "./logical-navigation";
import { routeTree } from "./routeTree.gen";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

function renderPage(path = "/class-timetable") {
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({
      initialEntries: initialLogicalEntries(`https://tauri.localhost${path}`),
    }),
  });
  render(<RouterProvider router={router} />);
  return router;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listen).mockResolvedValue(() => undefined);
  vi.mocked(openUrl).mockResolvedValue(undefined);
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_auth_state") return "unauthenticated";
    if (command === "get_account_profile" || command === "get_user_data") return null;
    return undefined;
  });
});

afterEach(() => {
  cleanup();
  delete window.SzuApplicationBack;
});

describe("class timetable", () => {
  it("shows all rows from the official image without requiring authentication", async () => {
    renderPage();
    const table = await screen.findByRole("grid", { name: "深圳大学上课时间表" });
    const rows = within(table).getAllByRole("row");
    expect(
      within(rows[0])
        .getAllByRole("columnheader")
        .map((cell) => cell.textContent),
    ).toEqual(["时段", "节次", "时间"]);
    // Independent transcription of the source image, including its break and evening rows.
    expect(
      rows
        .slice(1)
        .map((row) => Array.from(row.querySelectorAll("td, th"), (cell) => cell.textContent)),
    ).toEqual([
      ["上午", "第一节", "8:30 — 9:10"],
      ["上午", "第二节", "9:15 — 9:55"],
      ["上午", "大课间", "20 分钟"],
      ["上午", "第三节", "10:15 — 10:55"],
      ["上午", "第四节", "11:00 — 11:40"],
      ["上午", "第五节", "11:45 — 12:25"],
      ["下午", "第六节", "13:30 — 14:10"],
      ["下午", "第七节", "14:15 — 14:55"],
      ["下午", "第八节", "15:00 — 15:40"],
      ["下午", "大课间", "20 分钟"],
      ["下午", "第九节", "16:00 — 16:40"],
      ["下午", "第十节", "16:45 — 17:25"],
      ["晚上", "第十一、十二节", "19:00 — 20:20"],
      ["晚上", "第十三、十四节", "20:30 — 21:45"],
    ]);
  });

  it("opens from the workbench and returns through the visible back button", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_auth_state")
        return { Authenticated: { studentId: "1234567890", name: "测试同学" } };
      if (command === "get_account_profile")
        return { studentId: "1234567890", campusCardNumber: "123456", hasPassword: true };
      if (command === "get_user_data") return { studentId: "1234567890", name: "测试同学" };
      return undefined;
    });
    const router = renderPage("/");
    const entry = await screen.findByRole("button", { name: /^上课时间表/ });
    expect(entry).toHaveTextContent("查看各节次上课时间");
    expect(screen.queryByRole("button", { name: /^课表/ })).not.toBeInTheDocument();
    fireEvent.click(entry);
    expect(await screen.findByRole("grid", { name: "深圳大学上课时间表" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/class-timetable");
    expect(screen.queryByText("上课时间表功能即将支持")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "返回工作台" }));
    expect(await screen.findByRole("heading", { name: "深大生活工作台" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/");
    expect(router.history.canGoBack()).toBe(false);
  });

  it("returns a direct link to the workbench root", async () => {
    const router = renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "返回工作台" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/"));
    expect(await screen.findByRole("heading", { name: "请完成统一认证" })).toBeInTheDocument();
    expect(router.history.canGoBack()).toBe(false);
  });

  it("sends the page back action to Android and handles its committed navigation", async () => {
    const bridge = { postMessage: vi.fn(), onmessage: null } as NonNullable<
      Window["SzuApplicationBack"]
    >;
    window.SzuApplicationBack = bridge;
    const router = renderPage();
    const button = await screen.findByRole("button", { name: "返回工作台" });
    await waitFor(() => expect(bridge.onmessage).not.toBeNull());
    fireEvent.click(button);
    expect(bridge.postMessage).toHaveBeenCalledWith("back-request");
    expect(router.state.location.pathname).toBe("/class-timetable");
    await act(async () => bridge.onmessage?.({ data: "navigate-back" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/"));
    await waitFor(() => expect(bridge.postMessage).toHaveBeenLastCalledWith("system"));
  });

  it("opens the official source and allows retry after an opener failure", async () => {
    vi.mocked(openUrl).mockRejectedValueOnce(new Error("Unavailable"));
    const router = renderPage();
    const source = await screen.findByRole("button", { name: "来源：深圳大学教务部" });
    fireEvent.click(source);
    expect(await screen.findByRole("alert")).toHaveTextContent("无法打开系统浏览器，请稍后重试。");
    expect(openUrl).toHaveBeenCalledWith("https://jwb.szu.edu.cn/bszn1/sksjb.htm");
    expect(screen.getByRole("grid", { name: "深圳大学上课时间表" })).toBeInTheDocument();
    fireEvent.click(source);
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(openUrl).toHaveBeenCalledTimes(2);
    expect(router.state.location.pathname).toBe("/class-timetable");
  });
});
