import type { ReactNode } from "react";
import { LuArrowLeft } from "react-icons/lu";
import { useLogicalNavigation } from "../logical-navigation";

/**
 * 页面顶部应用栏，替代原来的 `ModuleHeader`。
 *
 * 返回按钮用原生 `<button>` 而不是 material 的 IconButton：后者把真实按钮
 * 放进 shadow root，`data-webview-navigation` 只落在宿主上，而
 * src/index.css 的隐藏规则与既有测试都要求它和可交互元素在同一个节点。
 * 这样也保证 Android 挂载原生工具栏时，页内返回按钮确实被隐藏。
 */
export function TopAppBar({
  title,
  description,
  eyebrow,
  backLabel = "返回工作台",
  actions,
}: {
  title: string;
  description?: string;
  eyebrow?: string;
  backLabel?: string;
  actions?: ReactNode;
}) {
  const navigation = useLogicalNavigation();

  return (
    <header className="top-app-bar">
      <div className="top-app-bar-row">
        <button
          type="button"
          data-webview-navigation
          className="top-app-bar-back"
          aria-label={backLabel}
          onClick={() => void navigation.back()}
        >
          <LuArrowLeft aria-hidden="true" size={20} />
        </button>
        <div className="top-app-bar-text">
          {eyebrow && (
            <span data-webview-navigation className="top-app-bar-eyebrow">
              {eyebrow}
            </span>
          )}
          <h1 data-webview-navigation className="top-app-bar-title">
            {title}
          </h1>
        </div>
        {actions && <div className="top-app-bar-actions">{actions}</div>}
      </div>
      {description && <p className="top-app-bar-description">{description}</p>}
    </header>
  );
}
