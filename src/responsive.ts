import { useEffect, useLayoutEffect, useState } from "react";

/**
 * 观察元素的内联尺寸是否达到阈值。
 *
 * 返回挂载用的 `ref` 回调与当前判定。用回调 ref 而不是 `RefObject`，是因为要测量的
 * 容器常常在数据到达后才渲染（例如 OJ 纸张）：只跑一次的 effect 拿 `ref.current`
 * 时它还是 `null`，之后元素挂上来也不会再测量。
 *
 * 判定是布尔值而不是原始尺寸：状态只在跨过阈值时变化，因此拖动窗口（niri 等平铺
 * 布局下尺寸会连续变化）不会每一像素都触发一次重渲染。
 *
 * 首帧测量在 `useLayoutEffect` 里同步完成，不会先渲染窄档再跳变。jsdom 没有布局
 * （rect 恒为 0），因此测试里稳定落到 `false`，与既有测试「总是走窄档」的假设一致。
 */
export function useInlineSizeAtLeast(minPx: number): {
  ref: (node: HTMLElement | null) => void;
  reached: boolean;
} {
  const [element, setElement] = useState<HTMLElement | null>(null);
  const [reached, setReached] = useState(false);

  useLayoutEffect(() => {
    if (!element) return;
    const measure = () => {
      const next = element.getBoundingClientRect().width >= minPx;
      setReached((previous) => (previous === next ? previous : next));
    };
    measure();
    if (typeof ResizeObserver === "function") {
      const observer = new ResizeObserver(measure);
      observer.observe(element);
      return () => observer.disconnect();
    }
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [element, minPx]);

  return { ref: setElement, reached };
}

/**
 * 视口高度是否低于阈值。
 *
 * 竖直空间预算取窗口高度而非某个容器的高度：容器高度随内容增长，不能代表
 * 「还剩多少竖直空间」。同样只在跨过阈值时更新状态。
 */
export function useViewportBlockSizeBelow(maxPx: number): boolean {
  const [below, setBelow] = useState(
    () => typeof window !== "undefined" && window.innerHeight < maxPx,
  );

  useLayoutEffect(() => {
    const measure = () => {
      const next = window.innerHeight < maxPx;
      setBelow((previous) => (previous === next ? previous : next));
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [maxPx]);

  return below;
}

/** 媒体查询是否匹配。 */
export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() =>
    typeof window !== "undefined" && typeof window.matchMedia === "function"
      ? window.matchMedia(query).matches
      : false,
  );

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const list = window.matchMedia(query);
    const update = () => setMatches(list.matches);
    update();
    list.addEventListener("change", update);
    return () => list.removeEventListener("change", update);
  }, [query]);

  return matches;
}
