import { setComponentDefaults } from "material/core";
// 只为带上 dialog 对 `ComponentConfigMap` 的类型增补（closeButton 等键），
// 以及注册 `setComponentDefaults("dialog", ...)` 的合法键名。
import type {} from "material/components/dialog";

/**
 * material 组件的全局默认值（进程内只需应用一次）。
 *
 * 对话框统一在标题栏左上角提供叉号关闭：
 * - 这里打开 `closeButton`，业务代码不再在 footer 里重复放「关闭」按钮；
 * - 左上角的位置由 index.css 对 `m-dialog::part(header-close)` 的覆盖决定；
 * - 确实不可关闭的阻塞/进度对话框在宿主上加 `dialog-no-close` 类
 *   （material 的布尔属性无法显式关闭全局默认）。
 */
export function applyMaterialDefaults(): void {
  setComponentDefaults("dialog", { closeButton: true });
}
