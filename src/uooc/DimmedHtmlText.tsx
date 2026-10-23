import { useEffect, useState, type ReactNode } from "react";

/**
 * 严谨匹配 HTML 标签的正则表达式：
 * 匹配 <tag>, </tag>, <tag attr="val">, <tag style="..."/>, <!-- comment --> 等，
 * 避免将数学比较符号（如 "x < y 且 z > 0"）误判为 HTML 标签。
 */
export const HTML_TAG_SPLIT_REGEX =
  /(<(?:\/?[a-zA-Z][a-zA-Z0-9:-]*(?:\s+[^>]*?)?\/?|!--[\s\S]*?--)>)/gi;

export const IS_EXACT_TAG_REGEX =
  /^<(?:\/?[a-zA-Z][a-zA-Z0-9:-]*(?:\s+[^>]*?)?\/?|!--[\s\S]*?--)>$/i;

const STORAGE_KEY = "uooc_show_html_tags";

// 跨组件事件分发，保证不同 Modal 和组件之间实时同步开关状态
const listeners = new Set<(show: boolean) => void>();

export function getStoredHtmlTagVisibility(): boolean {
  try {
    const val = localStorage.getItem(STORAGE_KEY);
    return val !== null ? val === "true" : true; // 默认显示淡化标签
  } catch {
    return true;
  }
}

export function setStoredHtmlTagVisibility(show: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(show));
  } catch {
    // Ignore
  }
  listeners.forEach((listener) => listener(show));
}

/**
 * 监听与控制是否显示淡化 HTML 标签的 Hook
 */
export function useHtmlTagVisibility(): [boolean, (show: boolean) => void] {
  const [show, setShow] = useState<boolean>(getStoredHtmlTagVisibility);

  useEffect(() => {
    const handleUpdate = (newVal: boolean) => setShow(newVal);
    listeners.add(handleUpdate);
    return () => {
      listeners.delete(handleUpdate);
    };
  }, []);

  const update = (newVal: boolean) => {
    setShow(newVal);
    setStoredHtmlTagVisibility(newVal);
  };

  return [show, update];
}

/**
 * 将含有 HTML 标签的文本进行切分渲染：
 * HTML 标签（如 <p>、<br>、<span style="..."> 等）可选择以 dim 暗淡虚化的字体展现，
 * 真实文本内容以正常样式高亮呈现。
 * 通过 showTags 参数或全局 useHtmlTagVisibility 开关可调整是否显示淡化标签。
 */
export function DimmedHtmlText({
  text,
  className,
  showTags,
}: {
  text?: string | null;
  className?: string;
  showTags?: boolean;
}): ReactNode {
  const [globalShowTags] = useHtmlTagVisibility();
  const visible = showTags !== undefined ? showTags : globalShowTags;

  if (!text) return null;

  const parts = text.split(HTML_TAG_SPLIT_REGEX);

  return (
    <span className={className}>
      {parts.map((part, index) => {
        if (!part) return null;
        if (IS_EXACT_TAG_REGEX.test(part)) {
          if (!visible) return null;
          return (
            <span
              key={index}
              data-dimmed-tag="true"
              className="text-muted/40 font-mono text-[0.85em] select-none inline-block mx-0.5"
            >
              {part}
            </span>
          );
        }
        return <span key={index}>{part}</span>;
      })}
    </span>
  );
}
