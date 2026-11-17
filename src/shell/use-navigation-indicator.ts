import { useEffect, useState } from "react";
import type { NavigationIndicatorBox } from "./navigation-indicator";
import { navigationIndicatorBox } from "./navigation-indicator";

/**
 * material 的选中指示器：`aria-current="page"` 的 item 下的 `[part~="indicator"]`。
 * parts 由 material 自己写到 shadow root 的节点上，比内部 BEM 类名稳定；
 * `.mtrl-navigation-bar__indicator` 只作为旧版本材料的兜底。
 *
 * 必须同时限定 `aria-current`：四项各有一个指示器，只按 part 取会永远拿到
 * 第一项。
 */
const INDICATOR_SELECTOR = '[aria-current="page"] [part~="indicator"]';
const INDICATOR_FALLBACK = ".mtrl-navigation-bar__item--active .mtrl-navigation-bar__indicator";

function findIndicator(host: Element): Element | null {
  const root = host.shadowRoot;
  if (!root) return null;
  return root.querySelector(INDICATOR_SELECTOR) ?? root.querySelector(INDICATOR_FALLBACK);
}

/**
 * 测量 `.app-nav-bar` 里 material 选中指示器的位置，供自绘滑动药丸跟随。
 *
 * 依赖 material 的公开结构（shadow root 里的指示器 + CSS parts），因此
 * 每次都可能量不到：宿主还没升级、页面全屏隐藏时 rect 全为 0。这时返回
 * `null`，调用方不渲染药丸。
 *
 * 重新测量的时机：
 * - `bar` / `shell` 节点本身（用回调 ref 传入）：首次挂载与宽窄屏切换；
 * - `active` 变化：点击后 material 会先同步移动自己的指示器，药丸据此
 *   立刻起步，不等路由切换完成；
 * - 选中项的 `change` 事件：覆盖 `active` 没变但材料内部状态已变的路径；
 * - ResizeObserver 与窗口尺寸变化：横竖屏、分屏、系统字号变化会改变布局；
 * - 字体就绪：标签字体切换后会改变 item 宽度。
 *
 * 传节点而不是 ref 对象：shell 与 material 宿主是两张不同的分支，回调 ref
 * 能保证节点一变就重新挂观测器，也避免把 ref 对象当作依赖。
 */
export function useNavigationIndicatorBox(
  bar: HTMLElement | null,
  shell: HTMLElement | null,
  active: string,
): NavigationIndicatorBox | null {
  const [box, setBox] = useState<NavigationIndicatorBox | null>(null);

  useEffect(() => {
    if (!bar || !shell) {
      setBox(null);
      return;
    }

    let frame = 0;
    const measure = () => {
      frame = 0;
      const indicator = findIndicator(bar);
      const style = getComputedStyle(shell);
      const next = indicator
        ? navigationIndicatorBox(indicator.getBoundingClientRect(), shell.getBoundingClientRect(), {
            borderTop: Number.parseFloat(style.borderTopWidth) || 0,
            borderLeft: Number.parseFloat(style.borderInlineStartWidth) || 0,
          })
        : null;
      // 每次测量都可能得到新对象，只在矩形真的变了时才更新 state。
      setBox((current) =>
        current === next ||
        (current &&
          next &&
          current.left === next.left &&
          current.top === next.top &&
          current.width === next.width &&
          current.height === next.height)
          ? current
          : next,
      );
    };
    const schedule = () => {
      if (frame) return;
      frame = requestAnimationFrame(measure);
    };

    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    observer?.observe(bar);
    observer?.observe(shell);
    const indicator = findIndicator(bar);
    if (indicator) observer?.observe(indicator);

    bar.addEventListener("change", schedule);
    window.addEventListener("resize", schedule);
    window.visualViewport?.addEventListener("resize", schedule);
    // 字体就绪会改变标签宽度；拿不到 `document.fonts` 的旧 WebView 直接跳过。
    void document.fonts?.ready.then(schedule);
    schedule();

    return () => {
      if (frame) cancelAnimationFrame(frame);
      observer?.disconnect();
      bar.removeEventListener("change", schedule);
      window.removeEventListener("resize", schedule);
      window.visualViewport?.removeEventListener("resize", schedule);
    };
  }, [bar, shell, active]);

  return box;
}
