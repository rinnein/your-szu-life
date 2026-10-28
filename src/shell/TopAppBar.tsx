import { type ReactNode } from "react";
import { TopAppBar as MaterialTopAppBar } from "material/react";
import { LuArrowLeft, LuSearch } from "react-icons/lu";
import { useLogicalNavigation } from "../logical-navigation";
import { openCommandPalette } from "../command-palette";

/**
 * 页面顶部应用栏。
 *
 * 用 material 的 `m-top-app-bar` 承载外壳与滚动状态（它按 window.scrollY
 * 自行切到 tonal 底色 + 抬升的已滚动态）；返回键、标题与动作都由本组件以
 * light DOM 放进槽位，因此它们仍能带 `data-webview-navigation`
 * ——src/index.css 的隐藏规则与既有测试都要求该属性和可交互元素在同一个节点。
 * （material 自己的 IconButton 会把按钮塞进 shadow root，做不到这一点。）
 *
 * 顶栏本身吸顶：`.top-app-bar` 是 sticky 外壳，页面内容从它下面滚过；
 * 说明文字留在外壳外面，随内容滚动。
 */
export function TopAppBar({
  title,
  description,
  eyebrow,
  leading,
  showBack = true,
  backLabel = "返回上一页",
  actions,
}: {
  title: string;
  description?: string;
  eyebrow?: string;
  leading?: ReactNode;
  showBack?: boolean;
  backLabel?: string;
  actions?: ReactNode;
}) {
  const navigation = useLogicalNavigation();

  return (
    <>
      <div className="top-app-bar">
        <MaterialTopAppBar
          aria-label="页面顶栏"
          leading={
            <span className="top-app-bar-leading">
              {showBack && (
                <button
                  type="button"
                  data-webview-navigation
                  className="top-app-bar-icon-button top-app-bar-back"
                  aria-label={backLabel}
                  onClick={() => void navigation.back()}
                >
                  <LuArrowLeft aria-hidden="true" size={20} />
                </button>
              )}
              {leading}
            </span>
          }
          trailing={
            <span className="top-app-bar-actions">
              <button
                type="button"
                className="top-app-bar-icon-button top-app-bar-search"
                aria-label="打开命令面板搜索"
                title="搜索功能或输入快捷指令（⌘ ⇧ P）"
                onClick={() => openCommandPalette()}
              >
                <LuSearch aria-hidden="true" size={20} />
              </button>
              {actions}
            </span>
          }
        >
          <span className="top-app-bar-text">
            {eyebrow && (
              // 装饰性眉题：不参与标题的可访问名称，标题即为页名。
              <span aria-hidden="true" data-webview-navigation className="top-app-bar-eyebrow">
                {eyebrow}
              </span>
            )}
            <span data-webview-navigation className="top-app-bar-title">
              {title}
            </span>
          </span>
        </MaterialTopAppBar>
      </div>
      {description && <p className="top-app-bar-description">{description}</p>}
    </>
  );
}
