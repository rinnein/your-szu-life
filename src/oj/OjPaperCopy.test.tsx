/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { mockElementRects } from "../test-layout";
import { copyRichText } from "../clipboard";
import { OjPaper } from "./OjPaper";
import type { Paper, Site } from "./types";

const { nav } = vi.hoisted(() => ({
  nav: { push: vi.fn(), replace: vi.fn(), back: vi.fn() },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("../clipboard", () => ({
  copyRichText: vi.fn().mockResolvedValue({ ok: true, rich: true }),
}));
vi.mock("../logical-navigation", () => ({
  useLogicalNavigation: () => nav,
  isChildDocument: () => false,
  nativeRuntime: () => false,
}));

const site: Site = {
  id: "test",
  name: "测试 OJ",
  url: "http://oj.test/",
  builtin: false,
  reminders: true,
  enabled: true,
  username: "student",
  session: "ready",
  message: "",
};
const paper: Paper = {
  account: "student",
  epoch: 1,
  writable: true,
  submitted: null,
  sourceUrl: "http://oj.test/#/contest/1/",
  contest: {
    id: "1",
    groupId: "2",
    title: "测验",
    description: "",
    begin: Date.now() - 10000,
    end: Date.now() + 3600000,
    now: Date.now(),
    isExam: false,
  },
  problems: [
    {
      id: "1",
      displayLabel: "A",
      kind: 1,
      title: "选择题",
      description: '<p>看图</p><img src="http://oj.test/media/a.png" alt="图A">',
      input: "",
      output: "",
      hint: "",
      timeLimit: 0,
      memoryLimit: 0,
      samples: [],
      choices: ["甲"],
      multiple: false,
      blanks: 0,
      languages: [],
      templates: {},
      ioMode: "stdio",
    },
  ],
  drafts: { "1:1": { code: "", language: "", answer: [], revision: 0, options: null } },
};

/** mock：checks[0] 是来源行判定，其余依次对应题面图片地址。 */
function mockBackend(options: { checks: boolean[]; imageDataUrl: string }) {
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    if (command === "get_oj_paper") return structuredClone(paper);
    if (command === "get_oj_records") return { records: [], uncertain: [] };
    if (command === "check_campus_targets") return options.checks;
    if (command === "load_oj_image") return options.imageDataUrl;
    return null;
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  window.matchMedia = vi.fn().mockImplementation((media) => ({
    media,
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  mockElementRects({});
  vi.mocked(copyRichText).mockResolvedValue({ ok: true, rich: true });
});
afterEach(cleanup);

async function openStatement() {
  render(<OjPaper site={site} contestId="1" tab="statement" desktop={false} port={27121} />);
  await screen.findByText("选择题");
}

it("inlines campus images and keeps the source line for public problems", async () => {
  mockBackend({ checks: [false, true], imageDataUrl: "data:image/png;base64,AAAA" });
  await openStatement();
  fireEvent.click(screen.getByRole("button", { name: "复制" }));
  await waitFor(() => expect(copyRichText).toHaveBeenCalled());
  const [markdown, html] = vi.mocked(copyRichText).mock.calls[0];
  expect(markdown).toContain("![图A](data:image/png;base64,AAAA)");
  expect(markdown).toContain("来源：http://oj.test/#/contest/1/");
  expect(html).toContain('<img src="data:image/png;base64,AAAA"');
  expect(html).toContain('来源：<a href="http://oj.test/#/contest/1/"');
});

it("omits the source line when it points to the campus network", async () => {
  mockBackend({ checks: [true, false], imageDataUrl: "data:image/png;base64,AAAA" });
  await openStatement();
  fireEvent.click(screen.getByRole("button", { name: "复制" }));
  await waitFor(() => expect(copyRichText).toHaveBeenCalled());
  const [markdown, html] = vi.mocked(copyRichText).mock.calls[0];
  expect(markdown).not.toContain("来源：");
  expect(html).not.toContain("来源：");
});

it("offers the MCP route when the inlined markdown exceeds the limit", async () => {
  mockBackend({
    checks: [false, true],
    imageDataUrl: `data:image/png;base64,${"A".repeat(2_500_000)}`,
  });
  await openStatement();
  fireEvent.click(screen.getByRole("button", { name: "复制" }));
  await screen.findByRole("alertdialog", { name: "图片过大" });
  // 正文（含体积）在 light DOM，alertdialog 表面在 shadow root 里。
  expect(screen.getByText(/2\.4 MB/)).toBeInTheDocument();
  expect(copyRichText).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole("button", { name: "仍然复制" }));
  await waitFor(() => expect(copyRichText).toHaveBeenCalled());
  await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
});

it("closes the oversized dialog without copying", async () => {
  mockBackend({
    checks: [false, true],
    imageDataUrl: `data:image/png;base64,${"A".repeat(2_500_000)}`,
  });
  await openStatement();
  fireEvent.click(screen.getByRole("button", { name: "复制" }));
  await screen.findByRole("alertdialog", { name: "图片过大" });

  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  expect(copyRichText).not.toHaveBeenCalled();
});

it("jumps to the MCP settings tab from the oversized dialog", async () => {
  mockBackend({
    checks: [false, true],
    imageDataUrl: `data:image/png;base64,${"A".repeat(2_500_000)}`,
  });
  await openStatement();
  fireEvent.click(screen.getByRole("button", { name: "复制" }));
  await screen.findByRole("alertdialog", { name: "图片过大" });

  fireEvent.click(screen.getByRole("button", { name: "前往设置" }));
  await waitFor(() => expect(nav.push).toHaveBeenCalledWith("/settings?tab=mcp"));
  await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  expect(copyRichText).not.toHaveBeenCalled();
});
