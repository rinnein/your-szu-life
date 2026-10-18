import type { ReactNode } from "react";

/**
 * 将含有 HTML 标签的文本进行切分渲染：
 * HTML 标签（如 <p>、<br>、<span> 等）以 dim 暗淡虚化的字体展现，
 * 真实文本内容以正常样式高亮呈现。
 */
export function DimmedHtmlText({
  text,
  className,
}: {
  text?: string | null;
  className?: string;
}): ReactNode {
  if (!text) return null;

  // 正则匹配所有常见 HTML 标签
  const tagRegex = /(<[^>]+>)/g;
  const parts = text.split(tagRegex);

  return (
    <span className={className}>
      {parts.map((part, index) => {
        if (part.startsWith("<") && part.endsWith(">")) {
          return (
            <span
              key={index}
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
