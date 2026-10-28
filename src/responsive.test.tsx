/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { useInlineSizeAtLeast, useViewportBlockSizeBelow } from "./responsive";
import { mockElementRects, mockViewportHeight } from "./test-layout";

/** 可手动触发的 ResizeObserver：jsdom 不做布局，尺寸变化得由测试驱动。 */
class ManualResizeObserver {
  static instances: ManualResizeObserver[] = [];
  callback: ResizeObserverCallback;

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
    ManualResizeObserver.instances.push(this);
  }
  observe() {}
  unobserve() {}
  disconnect() {}
  fire() {
    this.callback([], this as unknown as ResizeObserver);
  }
}

let renders = 0;

function WidthProbe({ min }: { min: number }) {
  const { ref, reached } = useInlineSizeAtLeast(min);
  renders += 1;
  return <div ref={ref} className="probe" data-reached={String(reached)} />;
}

function HeightProbe({ max }: { max: number }) {
  const below = useViewportBlockSizeBelow(max);
  renders += 1;
  return <div className="probe" data-below={String(below)} />;
}

const state = (container: HTMLElement, name: "data-reached" | "data-below") =>
  container.querySelector(".probe")?.getAttribute(name);

beforeEach(() => {
  renders = 0;
  ManualResizeObserver.instances = [];
  vi.stubGlobal("ResizeObserver", ManualResizeObserver);
  mockElementRects({});
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("按元素的实测宽度跨过阈值时翻转", () => {
  mockElementRects({ probe: 600 });
  const { container } = render(<WidthProbe min={880} />);
  expect(state(container, "data-reached")).toBe("false");

  mockElementRects({ probe: 1200 });
  act(() => ManualResizeObserver.instances[0].fire());
  expect(state(container, "data-reached")).toBe("true");
});

it("同一侧的尺寸变化几乎不触发重渲染", () => {
  mockElementRects({ probe: 1000 });
  const { container } = render(<WidthProbe min={880} />);
  expect(state(container, "data-reached")).toBe("true");

  // 模拟拖动窗口：全程都在阈值同一侧，连续变化 21 次。
  const before = renders;
  for (let width = 900; width <= 1400; width += 25) {
    mockElementRects({ probe: width });
    act(() => ManualResizeObserver.instances[0].fire());
  }
  expect(state(container, "data-reached")).toBe("true");
  // 逐像素存原始尺寸的话这里会是 21 次；存布尔只允许挂载后的那一次浪费。
  expect(renders - before).toBeLessThanOrEqual(1);
});

it("没有 ResizeObserver 时用窗口 resize 兜底", () => {
  vi.stubGlobal("ResizeObserver", undefined);
  mockElementRects({ probe: 600 });
  const { container } = render(<WidthProbe min={880} />);
  expect(state(container, "data-reached")).toBe("false");

  mockElementRects({ probe: 1200 });
  act(() => {
    window.dispatchEvent(new Event("resize"));
  });
  expect(state(container, "data-reached")).toBe("true");
});

it("无布局环境（rect 恒为 0）落到未达标", () => {
  const { container } = render(<WidthProbe min={880} />);
  expect(state(container, "data-reached")).toBe("false");
});

it("视口高度跨过阈值时翻转", () => {
  mockViewportHeight(900);
  const { container } = render(<HeightProbe max={560} />);
  expect(state(container, "data-below")).toBe("false");

  act(() => mockViewportHeight(400));
  expect(state(container, "data-below")).toBe("true");
});
