import type { ReactNode } from "react";
import { DimmedHtmlText } from "./DimmedHtmlText";
import { RemoteImage } from "../resources/remote";

export type RichImage = {
  pic_id?: string;
  picId?: string;
  url: string;
};

const MARKDOWN_IMAGE_SOURCE = String.raw`!\[([^\]\n]*)\]\(\s*(?:<([^>\n]+)>|([^()\s]+))(?:\s+(?:"[^"\n]*"|'[^'\n]*'|\([^)\n]*\)))?\s*\)`;

const PIC_REFERENCE_RE = /^#<([^>]+)>$/;
const DATA_IMAGE_RE = /^data:image\/(?:png|jpeg|jpg|gif|webp|bmp|avif);base64,/i;

function markdownImageRegex(): RegExp {
  return new RegExp(MARKDOWN_IMAGE_SOURCE, "gi");
}

export function countRichImages(text?: string | null): number {
  if (!text) return 0;
  let count = 0;
  const regex = markdownImageRegex();
  while (regex.exec(text)) count += 1;
  return count;
}

/**
 * 把富文本降级为单行纯文本：去掉图片语法（保留 alt）与 HTML 标签。
 * 用于选项 label、列表摘要等不渲染图片的位置。
 */
export function plainTextFromRich(text?: string | null): string {
  if (!text) return "";
  return text
    .replace(markdownImageRegex(), (_match, alt: string | undefined) => alt ?? "")
    .replace(/<[^>]*>/g, " ")
    .replace(/&nbsp;|&ensp;|&emsp;|&thinsp;|&#160;/gi, " ")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&quot;/gi, '"')
    .replace(/&#39;|&apos;/gi, "'")
    .replace(/&amp;/gi, "&")
    .replace(/\s+/g, " ")
    .trim();
}

function resolveImageSource(
  src: string,
  images: Map<string, string>,
  dataImages: string[] | undefined,
  dataIndex: number | undefined,
): string | null {
  // 空字符串是抓图失败的占位，继续回退到图片映射或 Markdown URL。
  const runtime = dataIndex !== undefined && dataImages ? dataImages[dataIndex] : undefined;
  if (runtime) {
    return runtime;
  }
  const trimmed = src.trim();
  const pic = PIC_REFERENCE_RE.exec(trimmed);
  if (pic) {
    return images.get(pic[1]) ?? null;
  }
  if (/^https?:\/\//i.test(trimmed) || DATA_IMAGE_RE.test(trimmed)) {
    return trimmed;
  }
  return null;
}

/**
 * 渲染 UOOC 题干/选项的富文本：
 * - Markdown 图片 `![说明](url)` 渲染为图片；
 * - `#<pic_id>` 通过题库返回的 images 映射解析（未上传时是 temp_pic_id，上传后是正式 pic_id）；
 * - 试卷运行时优先使用 dataImages（按图片出现顺序对应的 data URI）；
 * - 普通文本继续交给 DimmedHtmlText，兼容旧 HTML 数据与「淡化标签」开关。
 */
export function RichText({
  text,
  images,
  dataImages,
  imageOffset = 0,
  className,
  imageClassName,
}: {
  text?: string | null;
  images?: RichImage[] | null;
  dataImages?: string[] | null;
  imageOffset?: number;
  className?: string;
  imageClassName?: string;
}): ReactNode {
  if (!text) return null;

  const imageMap = new Map<string, string>();
  for (const image of images ?? []) {
    const key = image.pic_id ?? image.picId;
    if (key) imageMap.set(key, image.url);
  }

  const nodes: ReactNode[] = [];
  const regex = markdownImageRegex();
  let last = 0;
  let occurrence = 0;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(text))) {
    if (match.index > last) {
      nodes.push(<DimmedHtmlText key={`text-${last}`} text={text.slice(last, match.index)} />);
    }
    const alt = match[1] ?? "";
    const src = (match[2] ?? match[3] ?? "").trim();
    const resolved = resolveImageSource(
      src,
      imageMap,
      dataImages ?? undefined,
      imageOffset + occurrence,
    );
    occurrence += 1;
    if (resolved) {
      const imageClass =
        imageClassName ??
        "my-2 max-h-96 max-w-full rounded-lg border border-separator/40 object-contain";
      if (resolved.startsWith("data:") || resolved.startsWith("blob:")) {
        nodes.push(
          <img
            key={`image-${match.index}`}
            src={resolved}
            alt={alt || "题目图片"}
            loading="lazy"
            className={imageClass}
          />,
        );
      } else {
        // 远程图片统一走 Rust 资源代理，避免 WebView 的 CORS/防盗链限制。
        nodes.push(
          <RemoteImage
            key={`image-${match.index}`}
            src={resolved}
            kind="image"
            alt={alt || "题目图片"}
            loading="lazy"
            className={imageClass}
            fallback={alt ? <span>{alt}</span> : null}
          />,
        );
      }
    } else if (alt) {
      nodes.push(<span key={`alt-${match.index}`}>{alt}</span>);
    }
    last = regex.lastIndex;
  }

  if (last < text.length) {
    nodes.push(<DimmedHtmlText key={`text-${last}`} text={text.slice(last)} />);
  }

  return <span className={className}>{nodes}</span>;
}
