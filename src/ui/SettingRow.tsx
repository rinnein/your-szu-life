import type { ReactNode } from "react";

/**
 * 设置页与初始化向导共用的「标题 + 说明 + 控件」行。
 *
 * material 的 `ListItem` 尾部只接受字符串，放不进 `Switch`、`Button`，所以这类
 * 行以前在各页面里手写了七八遍，内边距、图标对齐与分隔线各不相同。这里把版式
 * 收成一处：左侧可选图标、中间标题与说明、右侧控件；行间分隔线由
 * `SettingList` 统一画。
 *
 * 行本身不加 ARIA 分组：控件的可访问名称由调用方给出（`aria-label` 写在
 * `Switch` / `Button` 上，与既有测试的查询方式一致）。若再给整行套一个同名
 * 的 `role="group"`，读屏会把同一个名字读两遍，按标签查询也会同时命中两个元素。
 */
export function SettingList({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={className ? `setting-list ${className}` : "setting-list"}>{children}</div>;
}

export function SettingRow({
  icon,
  title,
  description,
  children,
  className,
}: {
  /** 可选前置图标，20px，随说明文字使用次要色。 */
  icon?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  /** 右侧控件（`Switch`、`Button`、`Chip` 等）。 */
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div className={className ? `setting-row ${className}` : "setting-row"}>
      {icon != null && (
        <span className="setting-row-icon" aria-hidden="true">
          {icon}
        </span>
      )}
      <div className="setting-row-text">
        <div className="setting-row-title">{title}</div>
        {description != null && <p className="setting-row-description">{description}</p>}
      </div>
      {children != null && <div className="setting-row-control">{children}</div>}
    </div>
  );
}
