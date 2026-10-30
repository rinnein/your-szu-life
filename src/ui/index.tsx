/**
 * 自建 UI 与缺口组件入口。
 *
 * material/react 已有的基础组件（Button、Card、Dialog、Select、Switch、Checkbox、
 * Radio、Progress、LoadingIndicator、Tabs、TextField 等）均在业务代码中直接从
 * `material/react` 引入，本目录不再为它们保留任何包装或兼容层。
 * 本目录仅维护 material 未提供的缺口组件（Banner、DataTable、Disclosure、
 * NumberField、Pagination 等）与命令式提示桥接（toast）。
 */

/* ---------- 自建补齐 ---------- */
export { Avatar, Kbd, Pagination, ScrollShadow } from "./primitives";
export { NumberField } from "./NumberField";
export { Banner, type BannerStatus } from "./Banner";
export { Disclosure } from "./Disclosure";
export { Table } from "./DataTable";
export { toast, resetToasts, type ToastColor, type ToastOptions } from "./toast";
export { Chip, type ChipColor, type ChipVariant } from "./Chip";
export { AdaptiveTabs, type AdaptiveTabItem, type AdaptiveTabsProps } from "./AdaptiveTabs";
export { bindField } from "./field";
