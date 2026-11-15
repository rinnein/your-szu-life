import type { ReactNode } from "react";
import { createElement, useEffect, useRef } from "react";
import { MenuItem } from "material/react";
import "material/elements/css/menu";
import { defineMenu } from "material/elements";

// 菜单宿主元素由页面自己注册（React `<Menu>` 包装的 anchor 在挂载后写入会失效，
// 见 HomePage AccountMenu 的注释与 [[material-menu-anchor-positioning]]）。
// node 环境没有 customElements（纯 node 测试会 import 本模块的连带模块），跳过注册。
if (typeof customElements !== "undefined") {
  defineMenu();
}

export type ActionMenuItem =
  | { kind: "item"; value: string; label: ReactNode; disabled?: boolean; danger?: boolean }
  | { kind: "divider" }
  | { kind: "note"; label: ReactNode };

/**
 * 锚定到 `anchorId` 元素的溢出菜单。
 *
 * 直接渲染 `<m-menu>` 并让 React 在连接前写入 `anchor` 属性，首次挂载与重新
 * 挂载都能锚定正确（material 3.2.4 的定位只认创建时的 opener）。`open` 是
 * 反映属性，由菜单自己的 opener 与 `close` 事件驱动；不要给触发按钮再绑
 * `onClick`，两个来源会互相覆盖状态。
 */
export function ActionMenu({
  anchorId,
  open,
  onOpenChange,
  onSelect,
  items,
  ariaLabel,
}: {
  /** 触发元素的 id；该元素须在菜单的同一个 root 内。 */
  anchorId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (value: string) => void;
  items: ActionMenuItem[];
  ariaLabel?: string;
}) {
  const menuRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const menu = menuRef.current;
    if (!menu) return;
    const handleOpen = () => onOpenChange(true);
    const handleClose = () => onOpenChange(false);
    const handleSelect = (event: Event) => {
      onOpenChange(false);
      const value = (event as CustomEvent<{ value?: string }>).detail?.value;
      if (value) onSelect(value);
    };
    menu.addEventListener("open", handleOpen);
    menu.addEventListener("close", handleClose);
    menu.addEventListener("select", handleSelect);
    return () => {
      menu.removeEventListener("open", handleOpen);
      menu.removeEventListener("close", handleClose);
      menu.removeEventListener("select", handleSelect);
    };
  }, [onOpenChange, onSelect]);

  return createElement(
    "m-menu",
    {
      ref: menuRef,
      "aria-label": ariaLabel ?? "操作菜单",
      anchor: anchorId,
      position: "bottom-end",
      open: open ? "" : undefined,
    },
    ...items.map((item, index) => {
      if (item.kind === "divider") {
        return createElement(MenuItem, { key: `divider-${index}`, divider: true });
      }
      if (item.kind === "note") {
        // 只读说明行：禁用态且不给 value，select 不会外发。
        return createElement(MenuItem, { key: `note-${index}`, disabled: true }, item.label);
      }
      return createElement(MenuItem, { key: item.value, value: item.value, disabled: item.disabled }, item.label);
    }),
  );
}
