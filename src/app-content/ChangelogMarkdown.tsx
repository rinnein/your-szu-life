import { Markdown } from "@tanstack/markdown/react";
import type { ComponentPropsWithoutRef } from "react";
import { toast } from "../ui";
import { openPublicUrl } from "./external";

function safeHttpUrl(value: string): string | null {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:" ? url.href : null;
  } catch {
    // 相对/片段地址没有可信基准，宁可丢掉也不猜。
    return null;
  }
}

/** 只放行绝对 http(s)；其余（javascript:、file: 等）在解析阶段就被丢弃。 */
function urlTransform(url: string, _kind: "link" | "image", fallback: string) {
  return safeHttpUrl(fallback) ?? safeHttpUrl(url);
}

function ChangelogLink({ href, children, ...rest }: ComponentPropsWithoutRef<"a">) {
  // 脚注/页内锚点保持原样，其余链接交给系统浏览器：
  // WebView 里直接跳转会把应用本身导航走。
  if (!href || href.startsWith("#")) {
    return (
      <a href={href} {...rest}>
        {children}
      </a>
    );
  }
  return (
    <a
      href={href}
      {...rest}
      onClick={(event) => {
        event.preventDefault();
        void openPublicUrl(href).catch(() => {
          toast.danger("无法打开系统浏览器", { description: "请稍后重试。" });
        });
      }}
    >
      {children}
    </a>
  );
}

function ChangelogImage({ src, alt, ...rest }: ComponentPropsWithoutRef<"img">) {
  if (!src) return null;
  return <img src={src} alt={alt ?? ""} loading="lazy" referrerPolicy="no-referrer" {...rest} />;
}

const COMPONENTS = { a: ChangelogLink, img: ChangelogImage };

/**
 * 更新提醒 / 关于页的 Markdown 更新日志。
 *
 * 用 TanStack Markdown 的 React 适配器直接产出 React 节点，不经过
 * `dangerouslySetInnerHTML`；原始 HTML 默认不解析（`allowHtml` 关闭），
 * 链接与图片额外收敛到绝对 http(s)。
 */
export function ChangelogMarkdown({
  source,
  compact = false,
  className,
}: {
  source: string;
  compact?: boolean;
  className?: string;
}) {
  return (
    <div
      className={className ? `markdown-body ${className}` : "markdown-body"}
      data-size={compact ? "compact" : undefined}
    >
      <Markdown components={COMPONENTS} urlTransform={urlTransform}>
        {source}
      </Markdown>
    </div>
  );
}
