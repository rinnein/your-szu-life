import { useInlineSizeAtLeast, useViewportBlockSizeBelow } from "../responsive";

/**
 * 纸张容器放得下 240px 停靠侧栏所需的宽度：扣掉侧栏与间距后仍留约 600px 题面。
 * 这是按元素宽度而非视口宽度判断 —— 视口 900px 起会出现 `.app-rail`，Android 上
 * 又是另一套尺寸，只有容器实测宽度才等于侧栏/抽屉真正可用的宽度。
 */
export const OJ_DOCK_MIN_INLINE = 880;
/**
 * 低于此窗口高度时，顶部横条会挤掉题面区域一整行，改用触发按钮 + 抽屉。
 * 取窗口高度是因为纸张高度随题面内容增长，不能代表可用的竖直空间。
 */
export const OJ_COMPACT_MAX_BLOCK = 560;
/** 详情区宽到能并排显示题面与作答所需的宽度。 */
export const OJ_SPLIT_MIN_INLINE = 900;

/**
 * 题目选择器的档位。
 *
 * - `docked`：常开的 material Drawer，与题面并排停靠。
 * - `strip`：顶部横向 Tabs 条（每个 tab 带判定胶囊）。
 * - `sheet`：当前题目触发按钮 + 模态 Drawer。
 */
export type OjLayout = "docked" | "strip" | "sheet";

/** 宽度优先于高度：宽而矮的窗口仍用停靠侧栏，不白占一整行。 */
export function ojLayout(dockable: boolean, compact: boolean): OjLayout {
  if (dockable) return "docked";
  return compact ? "sheet" : "strip";
}

/**
 * 纸张与详情区各自的档位，连同要挂到这两个容器上的 ref。
 *
 * 两个测量都返回布尔，因此拖动窗口只在跨过阈值时才重渲染 —— OjPaper 里挂着
 * CodeMirror/Monaco 与 KaTeX 渲染过的题面，逐像素重渲染代价很实在。
 */
export function useOjLayout(): {
  layout: OjLayout;
  reading: boolean;
  paperRef: (node: HTMLElement | null) => void;
  detailRef: (node: HTMLElement | null) => void;
} {
  const paper = useInlineSizeAtLeast(OJ_DOCK_MIN_INLINE);
  const detail = useInlineSizeAtLeast(OJ_SPLIT_MIN_INLINE);
  const compact = useViewportBlockSizeBelow(OJ_COMPACT_MAX_BLOCK);
  return {
    layout: ojLayout(paper.reached, compact),
    reading: detail.reached,
    paperRef: paper.ref,
    detailRef: detail.ref,
  };
}
