/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { CommandPalette, openCommandPalette } from "./CommandPalette";
import { COMMAND_LIST, filterCommands, getLastVisitedOjGroupUrl } from "./commands";
import {
  getCommandHistory,
  getFrequentCommands,
  getRecentCommands,
  recordCommandUsage,
} from "./history";

const { nav } = vi.hoisted(() => ({
  nav: { push: vi.fn(), replace: vi.fn(), back: vi.fn() },
}));

vi.mock("../logical-navigation", () => ({
  useLogicalNavigation: () => nav,
}));

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
});

afterEach(() => {
  cleanup();
});

describe("filterCommands", () => {
  it("matches commands by their shortcuts", () => {
    expect(filterCommands("time")[0].id).toBe("time");
    expect(filterCommands("oj")[0].id).toBe("oj");
    expect(filterCommands("ojzj")[0].id).toBe("ojzj");
    expect(filterCommands("uooc")[0].id).toBe("uooc");
    expect(filterCommands("mooc")[0].id).toBe("uooc");
    expect(filterCommands("net")[0].id).toBe("net");
    expect(filterCommands("xyw")[0].id).toBe("net");
    expect(filterCommands("card")[0].id).toBe("card");
    expect(filterCommands("xyk")[0].id).toBe("card");
    expect(filterCommands("cdl")[0].id).toBe("cdl");
    expect(filterCommands("cdf")[0].id).toBe("cdf");
  });

  it("matches commands by their full names", () => {
    expect(filterCommands("上课时间表")[0].id).toBe("time");
    expect(filterCommands("查电量")[0].id).toBe("cdl");
    expect(filterCommands("充电费")[0].id).toBe("cdf");
    expect(filterCommands("校园网")[0].id).toBe("net");
  });
});

describe("getLastVisitedOjGroupUrl", () => {
  it("returns /oj when no last visit recorded", () => {
    expect(getLastVisitedOjGroupUrl()).toBe("/oj");
  });

  it("returns group url when last visit is saved in localStorage", () => {
    localStorage.setItem(
      "oj_last_visited",
      JSON.stringify({ siteId: "test-site", groupId: "group-8" }),
    );
    expect(getLastVisitedOjGroupUrl()).toBe("/oj?site=test-site&group=group-8");
  });
});

describe("CommandPalette history & recommendations", () => {
  it("records usage and sorts recent and frequent commands", () => {
    recordCommandUsage("cdl");
    recordCommandUsage("cdl");
    recordCommandUsage("time");

    const history = getCommandHistory();
    expect(history.cdl.useCount).toBe(2);
    expect(history.time.useCount).toBe(1);

    const frequent = getFrequentCommands(COMMAND_LIST);
    expect(frequent[0].id).toBe("cdl");
    expect(frequent[1].id).toBe("time");

    const recent = getRecentCommands(COMMAND_LIST);
    expect(recent[0].id).toBe("time"); // time was recorded last
    expect(recent[1].id).toBe("cdl");
  });
});

describe("CommandPalette UI & interaction", () => {
  it("opens on Command+Shift+P, highlights selected item with marker, and executes command", async () => {
    render(<CommandPalette />);

    expect(screen.queryByLabelText("命令面板搜索框")).not.toBeInTheDocument();

    // Trigger open via Command+Shift+P (metaKey)
    fireEvent.keyDown(window, { key: "P", shiftKey: true, metaKey: true });

    const searchInput = await screen.findByLabelText("命令面板搜索框");
    expect(searchInput).toBeInTheDocument();

    // Initial selected option has aria-selected="true" and primary highlight styling on icon & title
    const options = screen.getAllByRole("option");
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(options[0].className).toContain("bg-primary");
    // Verify title and icon are styled in primary
    expect(options[0].querySelector("span.text-primary")).toBeInTheDocument();
    expect(options[0].querySelector("div.bg-primary")).toBeInTheDocument();
    expect(options[0]).not.toHaveTextContent("选中");

    // Press ArrowDown to change selection
    fireEvent.keyDown(searchInput, { key: "ArrowDown" });
    expect(options[1]).toHaveAttribute("aria-selected", "true");
    expect(options[1].className).toContain("bg-primary");
    expect(options[1].querySelector("span.text-primary")).toBeInTheDocument();
    expect(options[1].querySelector("div.bg-primary")).toBeInTheDocument();
    expect(options[0]).toHaveAttribute("aria-selected", "false");

    // Type shortcut "cdl"
    fireEvent.change(searchInput, { target: { value: "cdl" } });

    const cdlOption = await screen.findByRole("option", { name: /查电量/ });
    expect(cdlOption).toBeInTheDocument();
    expect(cdlOption).toHaveAttribute("aria-selected", "true");
    expect(cdlOption.className).toContain("bg-primary");
    expect(cdlOption).not.toHaveTextContent("选中");

    fireEvent.click(cdlOption);

    // Navigates and closes
    await waitFor(() => {
      expect(nav.push).toHaveBeenCalledWith("/campus-payments/electricity");
      expect(screen.queryByLabelText("命令面板搜索框")).not.toBeInTheDocument();
    });

    // Recorded usage
    expect(getCommandHistory().cdl.useCount).toBe(1);
  });

  it("opens via openCommandPalette() and supports keyboard navigation Enter", async () => {
    render(<CommandPalette />);

    openCommandPalette();

    const searchInput = await screen.findByLabelText("命令面板搜索框");
    expect(searchInput).toBeInTheDocument();

    // Type "oj"
    fireEvent.change(searchInput, { target: { value: "oj" } });

    // Press Enter to execute the first matched option (OJ)
    fireEvent.keyDown(searchInput, { key: "Enter" });

    await waitFor(() => {
      expect(nav.push).toHaveBeenCalledWith("/oj");
      expect(screen.queryByLabelText("命令面板搜索框")).not.toBeInTheDocument();
    });
  });

  it("executes ojzj by resolving the last visited class or fallback", async () => {
    localStorage.setItem("oj_last_visited", JSON.stringify({ siteId: "szu", groupId: "10" }));

    render(<CommandPalette />);
    openCommandPalette();

    const searchInput = await screen.findByLabelText("命令面板搜索框");
    fireEvent.change(searchInput, { target: { value: "ojzj" } });

    const option = await screen.findByRole("option", { name: /最近测验列表/ });
    fireEvent.click(option);

    await waitFor(() => {
      expect(nav.push).toHaveBeenCalledWith("/oj?site=szu&group=10");
    });
  });

  it("closes when pressing Escape", async () => {
    render(<CommandPalette />);
    openCommandPalette();

    const searchInput = await screen.findByLabelText("命令面板搜索框");
    expect(searchInput).toBeInTheDocument();

    fireEvent.keyDown(searchInput, { key: "Escape" });

    await waitFor(() => {
      expect(screen.queryByLabelText("命令面板搜索框")).not.toBeInTheDocument();
    });
  });
});
