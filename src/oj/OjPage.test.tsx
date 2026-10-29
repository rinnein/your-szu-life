/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { shadowOf, typeInto, waitForUpgrade } from "../ui/testing";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { ojProgressCache, progressScope } from "./progress-store";
import { clearListCache } from "./list-cache";
import { listen } from "@tauri-apps/api/event";
import OjPage, { SiteEditor } from "./OjPage";
import { OjPaper } from "./OjPaper";
import { mockElementRects, mockViewportHeight } from "../test-layout";
import type { Paper, Site } from "./types";
const { search, nav, auth } = vi.hoisted(() => ({
  search: {
    site: undefined as string | undefined,
    group: undefined as string | undefined,
    contest: undefined as string | undefined,
    problem: undefined as string | undefined,
    tab: "statement",
  },
  nav: { push: vi.fn(), replace: vi.fn(), back: vi.fn() },
  auth: {
    state: { Authenticated: { studentId: "20260001" } },
    profile: undefined,
    userData: undefined,
  },
}));
vi.mock("../auth", () => ({ useAuth: () => auth }));
vi.mock("@tanstack/react-router", () => ({ useSearch: () => search }));
vi.mock("../logical-navigation", () => ({ useLogicalNavigation: () => nav }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class MockChannel {
    onmessage = null;
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
}));
vi.mock("./CodeEditor", () => ({
  CodeEditor: ({
    value,
    onChange,
    disabled,
  }: {
    value: string;
    onChange: (v: string) => void;
    disabled: boolean;
  }) => (
    <textarea
      aria-label="代码编辑器"
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
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
      description: "选一个",
      input: "",
      output: "",
      hint: "",
      timeLimit: 0,
      memoryLimit: 0,
      samples: [],
      choices: ["甲", "乙"],
      multiple: false,
      blanks: 0,
      languages: [],
      templates: {},
      ioMode: "stdio",
    },
  ],
  drafts: { "1:1": { code: "", language: "", answer: [], revision: 0, options: null } },
};
beforeEach(() => {
  ojProgressCache.clear();
  clearListCache();
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
  // 默认按「无布局」起步（宽度 0、窗口高 768），档位测试自己按需打桩。
  mockElementRects({});
  vi.clearAllMocks();
  search.site = undefined;
  search.group = undefined;
  search.contest = undefined;
  search.problem = undefined;
  auth.state.Authenticated.studentId = "20260001";
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_oj_snapshot")
      return {
        sites: [
          site,
          {
            ...site,
            id: "other",
            name: "未启用 OJ",
            enabled: false,
            username: "",
            session: "disabled",
          },
        ],
        notices: [],
        desktop: true,
        companionPort: 27121,
      };
    if (command === "get_oj_contests")
      return [
        { ...paper.contest, id: "1" },
        { ...paper.contest, id: "2", title: "未来测验", begin: paper.contest.now + 10000 },
      ];
    if (command === "get_oj_groups") return [{ id: "2", name: "一班" }];
    if (command === "get_oj_paper") return structuredClone(paper);
    if (command === "get_oj_records") return { records: [], uncertain: [] };
    if (command === "submit_oj") return { id: 100 };
    return null;
  });
});
afterEach(cleanup);
it("separates configured sites and opens the class list", async () => {
  render(<OjPage />);
  expect(await screen.findByText("已配置")).toBeTruthy();
  expect(screen.getByRole("heading", { name: "未配置" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "进入班级" }));
  expect(nav.push).toHaveBeenCalledWith("/study/oj?site=test");
});
it("shows stored username but never requests stored passwords", async () => {
  render(<OjPage />);
  await screen.findByText("测试 OJ");
  fireEvent.click(screen.getAllByRole("button", { name: "管理" })[0]);
  await screen.findByRole("dialog");
  expect((screen.getByLabelText(/密码/) as HTMLInputElement).value).toBe("");
  expect(screen.getByLabelText("账号")).toHaveProperty("value", "student");
});
it("loads classes through the public page", async () => {
  search.site = "test";
  render(<OjPage />);
  expect(await screen.findByText("一班")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "查看测验" }));
  expect(nav.push).toHaveBeenCalledWith("/study/oj?site=test&group=2");
});
it("reuses the cached class list when the page is reopened", async () => {
  search.site = "test";
  const first = render(<OjPage />);
  expect(await screen.findByText("一班")).toBeTruthy();
  const groupCalls = () =>
    vi.mocked(invoke).mock.calls.filter(([command]) => command === "get_oj_groups").length;
  expect(groupCalls()).toBe(1);

  first.unmount();
  render(<OjPage />);
  expect(await screen.findByText("一班")).toBeTruthy();
  expect(groupCalls()).toBe(1);
});
it("mobile omits Competitive Companion", async () => {
  render(<OjPaper site={site} contestId="1" tab="answer" desktop={false} port={27121} />);
  await screen.findByText("单选");
  expect(screen.queryByText("本机接收端口")).toBeNull();
  expect(screen.queryByText("发送到本机工具")).toBeNull();
});
it("saves a choice locally before confirmed submission", async () => {
  render(<OjPaper site={site} contestId="1" tab="answer" desktop={false} port={27121} />);
  const choice = await screen.findByRole("radio", { name: "A. 甲" });
  fireEvent.click(choice);
  // 选项落定、且防抖保存真的发出之后再看提交：
  // material 的选择控件异步回写选中态，抢在同一帧提交会让
  // 「先存草稿」这一步没有内容可存。
  await waitFor(() => expect(choice).toBeChecked());
  // 「正在保存…」是组件自己给出的反馈，出现即说明这次选择已经进了草稿状态。
  await screen.findByText("正在保存…");
  fireEvent.click(screen.getByRole("button", { name: "提交非编程题答卷" }));
  expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "submit_oj")).toBe(false);
  fireEvent.click(await screen.findByRole("button", { name: "确认提交" }));
  await waitFor(() =>
    expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "submit_oj")).toBe(true),
  );
  const calls = vi.mocked(invoke).mock.calls;
  expect(calls.findIndex(([cmd]) => cmd === "save_oj_draft")).toBeLessThan(
    calls.findIndex(([cmd]) => cmd === "submit_oj"),
  );
  expect(calls.find(([cmd]) => cmd === "save_oj_draft")?.[1]).toMatchObject({
    input: { draft: { answer: [0] } },
  });
});
it("does not submit when saving the draft fails", async () => {
  const original = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((cmd, ...args) =>
    cmd === "save_oj_draft" ? Promise.reject("存储已满") : original(cmd, ...args),
  );
  render(<OjPaper site={site} contestId="1" tab="answer" desktop={false} port={27121} />);
  const choice = await screen.findByRole("radio", { name: "A. 甲" });
  fireEvent.click(choice);
  await waitFor(() => expect(choice).toBeChecked());
  // 选择进入草稿状态后再提交：提交时 flush 才会真的去写这份草稿并撞上错误。
  await screen.findByText("正在保存…");
  fireEvent.click(screen.getByRole("button", { name: "提交非编程题答卷" }));
  fireEvent.click(await screen.findByRole("button", { name: "确认提交" }));
  await screen.findByText("存储已满");
  expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "submit_oj")).toBe(false);
});

it("prefills the school ID but does not overwrite edits when authentication arrives", () => {
  auth.state.Authenticated.studentId = "";
  const props = { close: () => {}, saved: async () => {} };
  const view = render(<SiteEditor {...props} />);
  const acct = screen.getByLabelText("账号");
  console.log(
    "[OJ] acct tag:",
    acct.tagName,
    "type:",
    (acct as HTMLInputElement).type,
    "value:",
    (acct as HTMLInputElement).value,
  );
  typeInto(acct, "custom-user");
  console.log("[OJ] after type:", (acct as HTMLInputElement).value);
  auth.state.Authenticated.studentId = "20260001";
  view.rerender(<SiteEditor {...props} />);
  expect(screen.getByLabelText("账号")).toHaveProperty("value", "custom-user");
});
it("fills an untouched empty account when the school ID loads", () => {
  auth.state.Authenticated.studentId = "";
  const props = { close: () => {}, saved: async () => {} };
  const view = render(<SiteEditor {...props} />);
  auth.state.Authenticated.studentId = "20260002";
  view.rerender(<SiteEditor {...props} />);
  expect(screen.getByLabelText("账号")).toHaveProperty("value", "20260002");
});
it("renders contests as list rows and highlights the running one", async () => {
  search.site = "test";
  search.group = "2";
  render(<OjPage />);
  const list = await screen.findByRole("list", { name: "测验列表" });
  const host =
    list.tagName.toLowerCase() === "m-list" ? list : (list.getRootNode() as ShadowRoot).host;
  await waitFor(() =>
    expect((host as HTMLElement).shadowRoot?.querySelectorAll('[role="listitem"]')).toHaveLength(2),
  );
  // 进行中的测验由 List 的选中态表达；行内容在 shadow root 内。
  const rows = (host as HTMLElement).shadowRoot!.querySelectorAll('[role="listitem"]');
  expect(rows[0].textContent).toContain("未来测验");
  expect(
    (host as HTMLElement).shadowRoot!.querySelector(".mtrl-list__item--selected"),
  ).not.toBeNull();
});
it("saves compilation settings immediately for the instance and retains them on another problem", async () => {
  const programming: Paper = {
    ...paper,
    preferences: { language: "cpp", optionsByLanguage: {}, revision: 0 },
    problems: [1, 2].map((id) => ({
      ...paper.problems[0],
      id: String(id),
      displayLabel: id === 1 ? "A" : "B",
      kind: 0,
      title: `题目 ${id}`,
      languages: ["cpp", "py"],
      choices: [],
    })),
    drafts: {
      "0:1": { code: "first", language: "cpp", options: null, answer: "", revision: 1 },
      "0:2": { code: "second", language: "cpp", options: null, answer: "", revision: 1 },
    },
  };
  const previous = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === "get_oj_paper") return programming;
    if (cmd === "save_oj_preferences")
      return {
        language: "cpp",
        optionsByLanguage: { cpp: { version: "c++20", enableAsan: true, enableLsan: false } },
        revision: 1,
      };
    return previous(cmd, args);
  });
  const props = { site, contestId: "1", tab: "answer", desktop: false, port: 27121 };
  const view = render(<OjPaper {...props} selected="0:1" />);
  const asan = await screen.findByRole("switch", { name: /ASan ·/ });
  fireEvent.click(asan);
  // material 的开关在点击后异步回写选中态：不等它落定就点下一步，
  // 保存动作会读到还没更新的选项。
  await waitFor(() => expect(asan).toBeChecked());
  await waitFor(() =>
    expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "save_oj_preferences")).toBe(true),
  );
  await screen.findByText("实例编译配置已保存");
  expect(
    vi.mocked(invoke).mock.calls.find(([cmd]) => cmd === "save_oj_preferences")?.[1],
  ).toMatchObject({ input: { siteId: "test", language: "cpp", options: { enableAsan: true } } });
  view.rerender(<OjPaper {...props} selected="0:2" />);
  expect(screen.getByRole("switch", { name: /ASan ·/ })).toHaveProperty("checked", true);
  expect(screen.getByLabelText("代码编辑器")).toHaveProperty("value", "second");
});
it("keeps the newly selected instance language when switching questions", async () => {
  const programming: Paper = {
    ...paper,
    preferences: { language: "cpp", optionsByLanguage: {}, revision: 0 },
    problems: [1, 2].map((id) => ({
      ...paper.problems[0],
      id: String(id),
      displayLabel: id === 1 ? "A" : "B",
      kind: 0,
      title: `题目 ${id}`,
      languages: ["cpp", "py"],
      choices: [],
    })),
    drafts: {
      "0:1": { code: "first", language: "cpp", options: null, answer: "", revision: 1 },
      "0:2": { code: "second", language: "cpp", options: null, answer: "", revision: 1 },
    },
  };
  const previous = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === "get_oj_paper") return programming;
    if (cmd === "save_oj_preferences")
      return { language: "py", optionsByLanguage: {}, revision: 1 };
    if (cmd === "get_oj_draft")
      return { code: "print(1)", language: "py", options: null, answer: "", revision: 2 };
    return previous(cmd, args);
  });
  const props = { site, contestId: "1", tab: "answer", desktop: false, port: 27121 };
  const view = render(<OjPaper {...props} selected="0:1" />);
  const combobox = await screen.findByRole("combobox", { name: "编程语言" });
  const select = (combobox.getRootNode() as ShadowRoot).host || combobox;
  fireEvent(select, new CustomEvent("change", { detail: { value: "py" } }));
  await screen.findByText("实例编译配置已保存");
  view.rerender(<OjPaper {...props} selected="0:2" />);
  expect(screen.getByRole("combobox", { name: "编程语言" })).toHaveValue("Python 3");
  expect(screen.getByLabelText("代码编辑器")).toHaveProperty("value", "print(1)");
});
it("keeps Companion controls inside global configuration", async () => {
  render(<OjPage />);
  await screen.findByText("测试 OJ");
  expect(screen.queryByText("cph-ng 直接提交")).toBeNull();
  expect(screen.queryByLabelText("本机接收端口")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "全局配置" }));
  expect(await screen.findByText("OJ 全局配置")).toBeTruthy();
  expect(screen.getByText("cph-ng 直接提交")).toBeTruthy();
  expect(screen.getByLabelText("本机接收端口")).toHaveProperty("value", "27121");
});
it("adds helpful placeholders for class and contest lookup", async () => {
  search.site = "test";
  const view = render(<OjPage />);
  await screen.findByText("一班");
  expect(screen.getAllByPlaceholderText("输入班级名称")[0]).toBeTruthy();
  search.group = "2";
  view.rerender(<OjPage />);
  // 宿主与内部输入共用同一占位文案，取首个。
  const [contestSearch] = await screen.findAllByPlaceholderText("输入测验名称");
  expect(contestSearch).toBeTruthy();
});
it("sends using the latest global port without a per-paper port input", async () => {
  const programming: Paper = {
    ...paper,
    problems: [{ ...paper.problems[0], kind: 0, languages: ["cpp"] }],
    drafts: {
      "0:1": { code: "int main(){}", language: "cpp", answer: "", options: null, revision: 1 },
    },
  };
  const previous = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) =>
    cmd === "get_oj_paper"
      ? programming
      : cmd === "send_oj_companion"
        ? { sent: 1 }
        : previous(cmd, args),
  );
  const view = render(<OjPaper site={site} contestId="1" tab="records" desktop port={27121} />);
  await screen.findByText("A. 选择题", { selector: "h2" });
  view.rerender(<OjPaper site={site} contestId="1" tab="records" desktop port={27300} />);
  expect(screen.queryByLabelText("本机接收端口")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "发送全部题目到CPH" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith(
      "send_oj_companion",
      expect.objectContaining({ port: 27300 }),
    ),
  );
});

it.each([
  { group: undefined, contest: undefined, problem: undefined },
  { group: "2", contest: undefined, problem: undefined },
  { group: "2", contest: "1", problem: undefined },
  { group: "2", contest: "1", problem: "1:1" },
])("does not show global configuration on OJ child pages: %j", async (location) => {
  search.site = "test";
  Object.assign(search, location);
  render(<OjPage />);
  await screen.findByRole("heading", { name: "测试 OJ", level: 1 });
  expect(screen.queryByRole("button", { name: "全局配置" })).toBeNull();
  expect(screen.queryByText("cph-ng 直接提交")).toBeNull();
  expect(screen.queryByLabelText("本机接收端口")).toBeNull();
});

/** 两道编程题的测验：题目目录与切换器都只有多题时才有意义。 */
const twoCodingProblems: Paper = {
  ...paper,
  problems: [1, 2].map((id) => ({
    ...paper.problems[0],
    id: String(id),
    displayLabel: id === 1 ? "A" : "B",
    kind: 0,
    title: `题目 ${id}`,
    choices: [],
    languages: ["cpp"],
  })),
  drafts: {
    "0:1": { code: "first", language: "cpp", options: null, answer: "", revision: 1 },
    "0:2": { code: "second", language: "cpp", options: null, answer: "", revision: 1 },
  },
};

/** 第一题 AC、第二题 WA 的提交记录。 */
function serveRecords(coding: Paper) {
  const previous = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === "get_oj_paper") return coding;
    if (cmd === "get_oj_records")
      return {
        records: [
          { id: 11, problem: 1, status: 0 },
          { id: 12, problem: 2, status: -1 },
        ],
        uncertain: [],
      };
    return previous(cmd, args);
  });
}

/** oj.css 依据这两个属性切换布局，断言值相等才算接上了。 */
const layoutOf = (container: HTMLElement) =>
  container.querySelector(".oj-paper")?.getAttribute("data-oj-layout");
const readingOf = (container: HTMLElement) =>
  container.querySelector(".oj-detail")?.getAttribute("data-oj-reading");

/**
 * 题目列表的行在 Drawer 的 shadow root 里（`<m-drawer-item>` 只是声明，不渲染内容），
 * 而 material 是在升级后按声明建行、并在声明属性变化时重建，所以要等升级再查。
 */
async function drawerRows(host: HTMLElement) {
  await waitForUpgrade(host);
  const root = host.shadowRoot as unknown as HTMLElement;
  return {
    inner: within(root),
    rows: () => Array.from(root.querySelectorAll<HTMLElement>(".mtrl-drawer__item")),
  };
}

/** 判定胶囊是 icon 槽里那个带行内样式的 span（胶囊在 shadow 内，只能靠行内样式着色）。 */
const pillOf = (row: HTMLElement) => row.querySelector<HTMLElement>(".mtrl-drawer__item-icon span");

it("容器放得下侧栏时用常开抽屉列出题目与判定", async () => {
  mockElementRects({ "oj-paper": 1600, "oj-detail": 1100 });
  serveRecords(twoCodingProblems);
  const props = { site, contestId: "1", tab: "records", desktop: false, port: 27121 };
  const view = render(<OjPaper {...props} selected="0:1" />);
  await screen.findByText("#11");
  expect(layoutOf(view.container)).toBe("docked");
  // 详情区自己宽到放得下题面与作答两栏。
  expect(readingOf(view.container)).toBe("split");
  expect(view.container.querySelector(".oj-switcher-strip")).toBeNull();
  expect(view.container.querySelector(".oj-switcher-compact")).toBeNull();

  const drawer = view.container.querySelector<HTMLElement>("m-drawer");
  expect(drawer).not.toBeNull();
  expect(drawer!.hasAttribute("modal")).toBe(false);
  // 宽度由 width 属性写成的内联样式供给，网格那一列（oj.css 的 240px）只是与之对齐。
  expect(drawer!.shadowRoot!.querySelector(".mtrl-drawer")?.getAttribute("style")).toContain(
    "--mtrl-drawer-width: 240px",
  );

  const { inner } = await drawerRows(drawer!);
  await waitFor(() =>
    expect(inner.getByRole("button", { name: /A\. 题目 1/ }).textContent).toContain("AC"),
  );
  const accepted = inner.getByRole("button", { name: /A\. 题目 1/ });
  const wrong = inner.getByRole("button", { name: /B\. 题目 2/ });
  expect(wrong.textContent).toContain("WA");
  // 判定仍是绿/红语义色：胶囊底色取 MD3 的 success / error 令牌。
  expect(pillOf(accepted)?.getAttribute("style")).toContain("--mtrl-sys-color-success");
  expect(pillOf(wrong)?.getAttribute("style")).toContain("--mtrl-sys-color-error");
  // 选中项由 Drawer 自己的 active 状态表达（value 与当前题目对齐）。
  expect(drawer!.shadowRoot!.querySelector(".mtrl-drawer__item--active")?.textContent).toContain(
    "A. 题目 1",
  );

  expect(screen.queryByText("#12")).toBeNull();
  view.rerender(<OjPaper {...props} selected="0:2" />);
  await screen.findByText("#12");
  expect(screen.queryByText("#11")).toBeNull();
});

it("容器窄而窗口高时只出现横向题目切换，胶囊带判定", async () => {
  mockElementRects({ "oj-paper": 600 });
  mockViewportHeight(900);
  serveRecords(twoCodingProblems);
  const view = render(
    <OjPaper site={site} contestId="1" tab="records" desktop={false} port={27121} />,
  );
  await screen.findByText("#11");
  expect(layoutOf(view.container)).toBe("strip");
  expect(readingOf(view.container)).toBe("stacked");
  expect(view.container.querySelector(".oj-switcher-strip")).not.toBeNull();
  expect(view.container.querySelector(".oj-switcher-compact")).toBeNull();
  expect(view.container.querySelector("m-drawer")).toBeNull();

  const tabs = shadowOf(view.container, "m-tabs");
  await waitFor(() =>
    expect(tabs.inner.getByRole("tab", { name: /A\. 题目 1/ }).textContent).toContain("AC"),
  );
  const accepted = tabs.inner.getByRole("tab", { name: /A\. 题目 1/ });
  const wrong = tabs.inner.getByRole("tab", { name: /B\. 题目 2/ });
  expect(wrong.textContent).toContain("WA");
  // 横条与抽屉用同一套彩色判定胶囊。
  expect(
    accepted.querySelector<HTMLElement>(".mtrl-button__icon span")?.getAttribute("style"),
  ).toContain("--mtrl-sys-color-success");
  expect(
    wrong.querySelector<HTMLElement>(".mtrl-button__icon span")?.getAttribute("style"),
  ).toContain("--mtrl-sys-color-error");
});

it("题目 Tabs 在中屏横向展示且在题目切换时自动调用 scrollIntoView 平滑居中", async () => {
  mockElementRects({ "oj-paper": 600 });
  mockViewportHeight(900);
  serveRecords(twoCodingProblems);
  const originalScrollIntoView = Element.prototype.scrollIntoView;
  const scrollIntoViewMock = vi.fn();
  Element.prototype.scrollIntoView = scrollIntoViewMock;

  try {
    const view = render(
      <OjPaper
        site={site}
        contestId="1"
        tab="records"
        desktop={false}
        port={27121}
        selected="0:1"
      />,
    );
    await screen.findByText("#11");
    const tabs = shadowOf(view.container, "m-tabs");
    expect(tabs).not.toBeNull();

    view.rerender(
      <OjPaper
        site={site}
        contestId="1"
        tab="records"
        desktop={false}
        port={27121}
        selected="0:2"
      />,
    );
    await screen.findByText("#12");
    expect(scrollIntoViewMock).toHaveBeenCalledWith(
      expect.objectContaining({ inline: "center", behavior: "smooth" }),
    );

    const strip = view.container.querySelector<HTMLDivElement>(".oj-switcher-strip")!;
    expect(strip).not.toBeNull();
    strip.scrollLeft = 0;
    const prevented = !fireEvent.wheel(strip, { deltaY: 120, deltaX: 0 });
    expect(prevented).toBe(true);
    expect(strip.scrollLeft).toBe(120);

    const lineWheel = new WheelEvent("wheel", {
      deltaY: 2,
      deltaX: 0,
      deltaMode: 1,
      cancelable: true,
    });
    strip.dispatchEvent(lineWheel);
    expect(strip.scrollLeft).toBe(186);
    expect(lineWheel.defaultPrevented).toBe(true);

    const horizontalPrevented = !fireEvent.wheel(strip, { deltaY: 0, deltaX: 50 });
    expect(horizontalPrevented).toBe(false);
    expect(strip.scrollLeft).toBe(186);
  } finally {
    Element.prototype.scrollIntoView = originalScrollIntoView;
  }
});

it("容器窄而窗口矮时用触发按钮加模态抽屉切换题目", async () => {
  mockElementRects({ "oj-paper": 600 });
  mockViewportHeight(500);
  serveRecords(twoCodingProblems);
  const view = render(
    <OjPaper site={site} contestId="1" tab="records" desktop={false} port={27121} />,
  );
  await screen.findByText("#11");
  expect(layoutOf(view.container)).toBe("sheet");
  expect(view.container.querySelector(".oj-switcher-compact")).not.toBeNull();
  expect(view.container.querySelector(".oj-switcher-strip")).toBeNull();
  // 窄屏的题目列表是模态抽屉，挂在 body 上而不是纸张里。
  expect(view.container.querySelector("m-drawer")).toBeNull();
  const drawer = document.querySelector<HTMLElement>("m-drawer")!;
  expect(drawer.hasAttribute("modal")).toBe(true);
  expect(drawer.hasAttribute("open")).toBe(false);

  fireEvent.click(screen.getByRole("button", { name: /题目 1\/2/ }));
  await waitFor(() => expect(drawer.hasAttribute("open")).toBe(true));
  const { inner } = await drawerRows(drawer);
  await waitFor(() =>
    expect(inner.getByRole("button", { name: /A\. 题目 1/ }).textContent).toContain("AC"),
  );
  fireEvent.click(inner.getByRole("button", { name: /B\. 题目 2/ }));
  await waitFor(() => expect(drawer.hasAttribute("open")).toBe(false));
  const calls = vi.mocked(nav.replace).mock.calls;
  const url = calls[calls.length - 1][0] as string;
  expect(new URLSearchParams(url.split("?")[1]).get("problem")).toBe("0:2");
});

it("只有一道题时不渲染任何题目切换器", async () => {
  mockElementRects({ "oj-paper": 600 });
  mockViewportHeight(900);
  const view = render(
    <OjPaper site={site} contestId="1" tab="statement" desktop={false} port={27121} />,
  );
  await screen.findByRole("heading", { name: "测验" });
  expect(view.container.querySelector(".oj-switcher-strip")).toBeNull();
  expect(view.container.querySelector(".oj-switcher-compact")).toBeNull();
  expect(view.container.querySelector("m-drawer")).toBeNull();
  expect(document.querySelector("m-drawer")).toBeNull();
});

it("loads cached CPH code into a clean editor and preserves unsaved edits on conflict", async () => {
  const coding: Paper = {
    ...paper,
    preferences: { language: "cpp", optionsByLanguage: {}, revision: 0 },
    problems: [{ ...paper.problems[0], kind: 0, title: "编程题", languages: ["cpp"] }],
    drafts: { "0:1": { code: "initial", language: "cpp", options: null, answer: "", revision: 1 } },
  };
  const previous = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) =>
    cmd === "get_oj_paper" ? coding : previous(cmd, args),
  );
  render(<OjPaper site={site} contestId="1" tab="answer" desktop={false} port={27121} />);
  const editor = await screen.findByLabelText("代码编辑器");
  const callback = vi.mocked(listen).mock.calls.find(([name]) => name === "oj-draft-changed")![1];
  const payload = {
    siteId: site.id,
    contestId: "1",
    problemKey: "0:1",
    account: paper.account,
    epoch: paper.epoch,
    draft: { ...coding.drafts["0:1"], code: "from-cph", revision: Date.now() + 1 },
  };
  await act(async () => callback({ event: "oj-draft-changed", id: 1, payload }));
  expect(editor).toHaveProperty("value", "from-cph");
  typeInto(editor, "unsaved-local");
  await act(async () =>
    callback({
      event: "oj-draft-changed",
      id: 1,
      payload: {
        ...payload,
        draft: { ...payload.draft, code: "new-cph", revision: Date.now() + 100 },
      },
    }),
  );
  expect(editor).toHaveProperty("value", "unsaved-local");
  expect(screen.getByRole("button", { name: "保留编辑器修改" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "载入 cph-ng 缓存" }));
  await waitFor(() => expect(editor).toHaveProperty("value", "new-cph"));
  expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "submit_oj")).toBe(false);
});

it("keeps the cached completed status visible while a paper reloads its records", async () => {
  const coding: Paper = {
    ...paper,
    problems: [{ ...paper.problems[0], kind: 0, languages: ["cpp"] }],
    drafts: {
      "0:1": { code: "int main(){}", language: "cpp", answer: "", revision: 1, options: null },
    },
  };
  ojProgressCache.seed(progressScope(site), "1", { accepted: 1, total: 1 });
  const previous = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === "get_oj_paper") return coding;
    if (cmd === "get_oj_records") return new Promise(() => {});
    return previous(cmd, args);
  });
  render(<OjPaper site={site} contestId="1" tab="answer" desktop={false} port={27121} />);
  expect(await screen.findByText("已完成")).toBeTruthy();
  expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "get_oj_progress")).toBe(false);
});

it("opens judging result in a modal dialog upon submitting code and can be closed", async () => {
  const programming: Paper = {
    ...paper,
    problems: [{ ...paper.problems[0], kind: 0, languages: ["cpp"] }],
    drafts: {
      "0:1": {
        code: "int main() { return 0; }",
        language: "cpp",
        options: null,
        answer: "",
        revision: 1,
      },
    },
  };
  const previous = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === "get_oj_paper") return programming;
    if (cmd === "submit_oj") return { id: 999, status: 0, language: "cpp" };
    return previous(cmd, args);
  });
  render(
    <OjPaper site={site} contestId="1" tab="answer" desktop={false} port={27121} selected="0:1" />,
  );
  const submitBtn = await screen.findByRole("button", { name: "提交代码" });
  fireEvent.click(submitBtn);

  // 模态弹窗打开
  const dialog = await waitFor(() => {
    const el = document.querySelector<HTMLElement>("m-dialog");
    expect(el).not.toBeNull();
    expect(el?.getAttribute("headline")).toContain("判题结果 #999");
    expect(el?.hasAttribute("open")).toBe(true);
    return el!;
  });
  expect(dialog).toBeInTheDocument();

  // 点击关闭按钮收起弹窗
  const closeBtn = screen.getByRole("button", { name: "关闭" });
  fireEvent.click(closeBtn);
  await waitFor(() => expect(document.querySelector("m-dialog")).toBeNull());
});
