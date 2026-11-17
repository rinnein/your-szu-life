import type { Destination } from "../shell/destinations";

/** 页面入场方式；`none` 不播放动画。 */
export type PageTransitionKind = "forward" | "back" | "none" | "fade-through";

type Action = "push" | "replace" | "back" | null;

/**
 * 根据逻辑导航动作与顶级部件的变化决定入场方式。
 *
 * - `null`（首次挂载）与 `push` 是建立页面层级，向前入场；
 * - `back` 是回退层级，向后入场；
 * - `replace` 有两种来源：顶级部件切换（底栏/rail，进入另一个部件）用
 *   M3 fade-through；同一逻辑页面内的 tab、筛选器切换不动画，否则每次
 *   切 tab 都会整页淡入，观感与 Android 原生不符。
 */
export function pageTransitionKind(
  action: Action,
  previous: Destination["value"] | null,
  next: Destination["value"],
): PageTransitionKind {
  if (action === "back") return "back";
  if (action !== "replace") return "forward";
  return previous !== null && previous !== next ? "fade-through" : "none";
}
