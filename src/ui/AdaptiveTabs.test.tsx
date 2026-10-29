/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { AdaptiveTabs, type AdaptiveTabItem } from "./AdaptiveTabs";
import { PageDrawerProvider } from "../shell/PageDrawerContext";
import { TopAppBar } from "../shell/TopAppBar";

const nav = vi.hoisted(() => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() }));
vi.mock("../logical-navigation", () => ({ useLogicalNavigation: () => nav }));

const sampleItems: AdaptiveTabItem[] = [
  { value: "overview", label: "概览" },
  { value: "details", label: "详情" },
  { value: "logs", label: "日志" },
];

describe("AdaptiveTabs Component & Drawer Auto-injection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    );
    Element.prototype.scrollIntoView = vi.fn();
  });

  afterEach(cleanup);

  it("当空间不足时默认呈现下拉触发器，点击展开浮层并支持选项切换", () => {
    const handleChange = vi.fn();
    render(
      <PageDrawerProvider>
        <AdaptiveTabs
          items={sampleItems}
          value="overview"
          onChange={handleChange}
          label="功能分类"
        />
      </PageDrawerProvider>,
    );

    // 默认 jsdom 宽度为 0，落入窄档下拉
    const trigger = screen.getByRole("button", { name: /功能分类.*概览/ });
    expect(trigger).toBeInTheDocument();
    expect(screen.queryByRole("listbox")).toBeNull();

    // 点击触发按钮展开 Popover 浮层
    fireEvent.click(trigger);
    const popover = screen.getByRole("listbox", { name: "功能分类" });
    expect(popover).toBeInTheDocument();

    // 点击选择“详情”
    const detailsOption = screen.getByRole("option", { name: /详情/ });
    fireEvent.click(detailsOption);

    expect(handleChange).toHaveBeenCalledWith("details");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("点击外部区域自动收起下拉浮层", () => {
    render(
      <PageDrawerProvider>
        <div>
          <button type="button" data-testid="outside">
            外部区域
          </button>
          <AdaptiveTabs items={sampleItems} value="overview" onChange={vi.fn()} label="测试" />
        </div>
      </PageDrawerProvider>,
    );

    const trigger = screen.getByRole("button", { name: /测试.*概览/ });
    fireEvent.click(trigger);
    expect(screen.getByRole("listbox")).toBeInTheDocument();

    // 点击外部
    fireEvent.pointerDown(screen.getByTestId("outside"));
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("当空间充足（minWidthForTabs=0）时直接呈现 Tabs 组件", () => {
    const handleChange = vi.fn();
    const { container } = render(
      <PageDrawerProvider>
        <AdaptiveTabs
          items={sampleItems}
          value="overview"
          onChange={handleChange}
          minWidthForTabs={0}
          ariaLabel="标签列表"
        />
      </PageDrawerProvider>,
    );

    const tabs = container.querySelector("m-tabs");
    expect(tabs).not.toBeNull();
    const tabElements = container.querySelectorAll("m-tab");
    expect(tabElements).toHaveLength(3);
  });

  it("自动向左上角抽屉注入内容，且 TopAppBar 自动显示汉堡抽屉按钮", () => {
    render(
      <PageDrawerProvider>
        <TopAppBar title="测试页面" />
        <AdaptiveTabs items={sampleItems} value="overview" onChange={vi.fn()} label="测试导航" />
      </PageDrawerProvider>,
    );

    // TopAppBar 自动渲染汉堡按钮
    const drawerButton = screen.getByRole("button", { name: "打开功能目录" });
    expect(drawerButton).toBeInTheDocument();

    // 抽屉被 portaled 到 document.body
    const drawer = document.querySelector<HTMLElement>("m-drawer");
    expect(drawer).not.toBeNull();
    expect(drawer?.hasAttribute("modal")).toBe(true);

    // 点击汉堡按钮打开抽屉
    fireEvent.click(drawerButton);
    expect(drawer?.hasAttribute("open")).toBe(true);
  });

  it("支持多实例注入并根据 label 自动划分 Divider 和 section 小标题", () => {
    const mainItems = [{ value: "m1", label: "主模块1" }];
    const subItems = [{ value: "s1", label: "子模块1" }];

    render(
      <PageDrawerProvider>
        <TopAppBar title="多模块测试" />
        <AdaptiveTabs items={mainItems} value="m1" onChange={vi.fn()} label="主功能" />
        <AdaptiveTabs items={subItems} value="s1" onChange={vi.fn()} label="附属功能" />
      </PageDrawerProvider>,
    );

    const drawer = document.querySelector<HTMLElement>("m-drawer")!;
    expect(drawer).not.toBeNull();

    // 验证 Drawer 内包含 section 标题与 divider
    const sectionHeaders = drawer.querySelectorAll('m-drawer-item[type="section"]');
    expect(sectionHeaders).toHaveLength(2);
    expect(sectionHeaders[0].getAttribute("label")).toBe("主功能");
    expect(sectionHeaders[1].getAttribute("label")).toBe("附属功能");

    const dividers = drawer.querySelectorAll('m-drawer-item[type="divider"]');
    expect(dividers).toHaveLength(1);
  });

  it("当 injectDrawer=false 时不向抽屉注入导航项", () => {
    render(
      <PageDrawerProvider>
        <TopAppBar title="无抽屉测试" />
        <AdaptiveTabs
          items={sampleItems}
          value="overview"
          onChange={vi.fn()}
          injectDrawer={false}
        />
      </PageDrawerProvider>,
    );

    // TopAppBar 不应出现汉堡按钮
    expect(screen.queryByRole("button", { name: "打开功能目录" })).toBeNull();
    // document.body 不应渲染 m-drawer
    expect(document.querySelector("m-drawer")).toBeNull();
  });

  it("支持 variant='segmented' 与 variant='pill' 并在宽屏呈现相应样式的分段/胶囊标签", () => {
    const handleChange = vi.fn();
    const { container, rerender } = render(
      <PageDrawerProvider>
        <AdaptiveTabs
          items={sampleItems}
          value="overview"
          onChange={handleChange}
          variant="segmented"
          size="sm"
          minWidthForTabs={0}
        />
      </PageDrawerProvider>,
    );

    const segmented = container.querySelector(".adaptive-tabs-segmented");
    expect(segmented).toBeInTheDocument();
    const buttons = container.querySelectorAll(".adaptive-tabs-segmented-btn");
    expect(buttons).toHaveLength(3);
    expect(buttons[0]).toHaveClass("adaptive-tabs-segmented-btn--selected");

    fireEvent.click(buttons[1]);
    expect(handleChange).toHaveBeenCalledWith("details");

    // 重新渲染为 pill 变体
    rerender(
      <PageDrawerProvider>
        <AdaptiveTabs
          items={sampleItems}
          value="overview"
          onChange={handleChange}
          variant="pill"
          minWidthForTabs={0}
        />
      </PageDrawerProvider>,
    );

    const pills = container.querySelector(".adaptive-tabs-pills");
    expect(pills).toBeInTheDocument();
    const pillButtons = container.querySelectorAll(".adaptive-tabs-pill");
    expect(pillButtons).toHaveLength(3);
  });

  it("支持 ReactNode 图标并在下拉浮层中正确渲染", () => {
    const itemsWithReactIcon: AdaptiveTabItem[] = [
      { value: "a", label: "代码", icon: <span data-testid="react-icon">IconA</span> },
      { value: "b", label: "文档", icon: "<span>HtmlIcon</span>" },
    ];

    render(
      <PageDrawerProvider>
        <AdaptiveTabs items={itemsWithReactIcon} value="a" onChange={vi.fn()} label="带图标" />
      </PageDrawerProvider>,
    );

    const trigger = screen.getByRole("button", { name: /带图标.*代码/ });
    fireEvent.click(trigger);

    const listbox = screen.getByRole("listbox");
    expect(listbox).toBeInTheDocument();
    expect(screen.getAllByTestId("react-icon")).toHaveLength(2);
    expect(within(listbox).getByText("HtmlIcon")).toBeInTheDocument();
  });

  it("当选项数量大于 8 或开启 searchable 时，在下拉模式提供即时搜索过滤与键盘上下回车导航", () => {
    const manyItems: AdaptiveTabItem[] = Array.from({ length: 12 }, (_, i) => ({
      value: `item-${i + 1}`,
      label: `题目 ${String.fromCharCode(65 + i)}`,
    }));

    const handleChange = vi.fn();
    render(
      <PageDrawerProvider>
        <AdaptiveTabs items={manyItems} value="item-1" onChange={handleChange} label="题库" />
      </PageDrawerProvider>,
    );

    const trigger = screen.getByRole("button", { name: /题库/ });
    fireEvent.click(trigger);

    // 自动出现搜索框
    const searchInput = screen.getByPlaceholderText("查找题库…");
    expect(searchInput).toBeInTheDocument();

    // 输入搜索文本进行过滤
    fireEvent.change(searchInput, { target: { value: "题目 C" } });
    const options = screen.getAllByRole("option");
    expect(options).toHaveLength(1);
    expect(options[0]).toHaveTextContent("题目 C");

    // 键盘导航：回车直接选中
    const popover = screen.getByRole("listbox");
    fireEvent.keyDown(popover, { key: "ArrowDown" });
    fireEvent.keyDown(popover, { key: "Enter" });

    expect(handleChange).toHaveBeenCalledWith("item-3");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("切换 Tab 时绝不调用 scrollIntoView，且保持页面原本的整体滚动位置", () => {
    const scrollIntoViewMock = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoViewMock;
    const handleChange = vi.fn();
    window.scrollY = 600;

    const { rerender } = render(
      <PageDrawerProvider>
        <AdaptiveTabs
          items={sampleItems}
          value="overview"
          onChange={handleChange}
          label="功能分类"
        />
      </PageDrawerProvider>,
    );

    rerender(
      <PageDrawerProvider>
        <AdaptiveTabs
          items={sampleItems}
          value="details"
          onChange={handleChange}
          label="功能分类"
        />
      </PageDrawerProvider>,
    );

    expect(scrollIntoViewMock).not.toHaveBeenCalled();
    expect(window.scrollY).toBe(600);
  });

  it("窄屏浮层中回车或ESC时带 preventScroll: true 聚焦触发器，不向上穿透触发页面滚回顶部", () => {
    const focusSpy = vi.spyOn(HTMLElement.prototype, "focus");
    render(
      <PageDrawerProvider>
        <AdaptiveTabs items={sampleItems} value="overview" onChange={vi.fn()} label="功能分类" />
      </PageDrawerProvider>,
    );

    const trigger = screen.getByRole("button", { name: /功能分类.*概览/ });
    fireEvent.click(trigger);
    const popover = screen.getByRole("listbox");

    // ESC 关闭
    fireEvent.keyDown(popover, { key: "Escape" });
    expect(focusSpy).toHaveBeenCalledWith(expect.objectContaining({ preventScroll: true }));
  });
});
