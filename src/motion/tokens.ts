/**
 * M3 Expressive 动效参数。
 *
 * material 组件自带弹簧动画，这里只覆盖应用自建的部分（页面转场、
 * 列表入场、展开区块），避免二次动画。
 */

/** M3 的 spatial 弹簧：用于位移与尺寸变化。 */
export const spatial = {
  fast: { type: "spring", stiffness: 500, damping: 36, mass: 0.8 },
  default: { type: "spring", stiffness: 380, damping: 34, mass: 0.9 },
  slow: { type: "spring", stiffness: 260, damping: 32, mass: 1 },
} as const;

/** M3 的 effects 弹簧：用于透明度与颜色。 */
export const effects = {
  fast: { duration: 0.12 },
  default: { duration: 0.2 },
  slow: { duration: 0.3 },
} as const;

/** 列表逐个入场的间隔（秒）。超过 8 项后不再递增，避免长列表拖沓。 */
export function staggerDelay(index: number, step = 0.03, max = 8): number {
  return Math.min(index, max) * step;
}
