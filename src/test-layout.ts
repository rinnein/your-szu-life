import { vi } from "vite-plus/test";

let rectSpy: ReturnType<typeof vi.spyOn> | undefined;

/**
 * 给元素一个可控的 `getBoundingClientRect()` 宽度。
 *
 * 布局档位由元素实测宽度决定，而 jsdom 不做布局（rect 恒为 0）。按 class 名给元素
 * 指定宽度后，测试就能确定性地选择停靠侧栏 / 横条 / 抽屉档位。重复调用会先撤掉上
 * 一次的打桩，避免叠加。
 */
export function mockElementRects(widths: Record<string, number>) {
  rectSpy?.mockRestore();
  rectSpy = vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (
    this: Element,
  ) {
    const classes = this.getAttribute("class");
    const matched = classes
      ? Object.keys(widths).find((name) => classes.split(/\s+/).includes(name))
      : undefined;
    const width = matched ? widths[matched] : 0;
    return {
      x: 0,
      y: 0,
      width,
      height: 0,
      top: 0,
      right: width,
      bottom: 0,
      left: 0,
      toJSON: () => ({}),
    } as DOMRect;
  });
  return rectSpy;
}

/** 设定视口高度；竖直空间预算按窗口高度计算。 */
export function mockViewportHeight(height: number) {
  Object.defineProperty(window, "innerHeight", {
    value: height,
    configurable: true,
    writable: true,
  });
  window.dispatchEvent(new Event("resize"));
}
