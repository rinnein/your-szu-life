import DOMPurify from "dompurify";
import Turndown from "turndown";
import { gfm } from "turndown-plugin-gfm";
import type { Paper, Problem } from "./types";
import { kindName } from "./types";
export function cleanHtml(html: string, base: string): string {
  const fragment = DOMPurify.sanitize(html, {
    RETURN_DOM_FRAGMENT: true,
    USE_PROFILES: { html: true, mathMl: true },
    FORBID_TAGS: ["form", "input", "button", "iframe", "style"],
    FORBID_ATTR: ["style"],
  });
  for (const el of fragment.querySelectorAll("[href], [src]")) {
    for (const attr of ["href", "src"]) {
      const value = el.getAttribute(attr);
      if (!value) continue;
      // 复制链路会把内网图片改写成 data URL 再转 markdown；只为 img 放行
      // 图片 data URI（与 load_oj_image 的 mime 白名单一致），其余仍然只认 http(s)。
      if (
        el.tagName === "IMG" &&
        attr === "src" &&
        /^data:image\/(png|jpeg|gif|webp);/i.test(value)
      )
        continue;
      try {
        const url = new URL(value, base);
        if (!["http:", "https:"].includes(url.protocol)) el.removeAttribute(attr);
        else el.setAttribute(attr, url.href);
      } catch {
        el.removeAttribute(attr);
      }
    }
    if (el.tagName === "A") {
      el.setAttribute("target", "_blank");
      el.setAttribute("rel", "noopener noreferrer");
    }
  }
  const wrapper = document.createElement("div");
  wrapper.append(fragment);
  return wrapper.innerHTML;
}
/** Suppress only empty descriptions and the site's bare deadline placeholder. */
export function hasContestDescription(html: string, base: string): boolean {
  const content = document.createElement("div");
  content.innerHTML = cleanHtml(html, base);
  if (content.querySelector("img[src], math, a[href], [data-latex]")) return true;
  const text = (content.textContent ?? "").replace(/[\s\u200b\ufeff]/g, "");
  return text.length > 0 && !/^截止时间[:：]?$/.test(text);
}
export function htmlMarkdown(html: string, base: string): string {
  const converter = new Turndown({
    headingStyle: "atx",
    codeBlockStyle: "fenced",
    bulletListMarker: "-",
  });
  converter.use(gfm);
  converter.addRule("math", {
    filter: (node) => node.nodeName === "MATH" || node.hasAttribute("data-latex"),
    replacement: (_content, node) => {
      const el = node as HTMLElement;
      const latex =
        el.getAttribute("data-latex") ??
        el.querySelector('annotation[encoding="application/x-tex"]')?.textContent;
      return latex ? ` $${latex}$ ` : (el.textContent ?? "");
    },
  });
  converter.addRule("pre", {
    filter: "pre",
    replacement: (_content, node) => `\n\n${fence(node.textContent ?? "")}\n\n`,
  });
  return converter.turndown(cleanHtml(html, base));
}
export function fence(text: string): string {
  const longest = Math.max(2, ...Array.from(text.matchAll(/`+/g), (m) => m[0].length));
  const mark = "`".repeat(longest + 1);
  return `${mark}text\n${text}${text.endsWith("\n") ? "" : "\n"}${mark}`;
}
export function problemMarkdown(
  p: Problem,
  paper: Paper,
  options: { sourceVisible?: boolean } = {},
): string {
  const { sourceVisible = true } = options;
  const parts = [`# ${p.displayLabel}. ${p.title}`];
  if (sourceVisible) parts.push(`来源：${paper.sourceUrl}`);
  parts.push(
    `测验：${paper.contest.title} · ${kindName[p.kind]}${p.score != null ? ` · ${p.score} 分` : ""}`,
  );
  if (p.timeLimit) parts.push(`时间限制：${p.timeLimit} 秒 · 内存限制：${p.memoryLimit} MB`);
  for (const [title, html] of [
    ["题目要求", p.description],
    ["输入说明", p.input],
    ["输出说明", p.output],
  ])
    if (html) parts.push(`## ${title}\n\n${htmlMarkdown(html, paper.sourceUrl)}`);
  if (p.choices.length)
    parts.push(
      `## 选项\n\n${p.choices.map((s, i) => `${String.fromCharCode(65 + i)}. ${htmlMarkdown(s, paper.sourceUrl)}`).join("\n\n")}`,
    );
  p.samples.forEach((s, i) =>
    parts.push(
      `## 样例 ${i + 1}\n\n### 输入\n\n${fence(s.input)}\n\n### 输出\n\n${fence(s.output)}`,
    ),
  );
  if (p.hint) parts.push(`## 提示\n\n${htmlMarkdown(p.hint, paper.sourceUrl)}`);
  for (const [language, template] of Object.entries(p.templates))
    parts.push(`## 代码模板（${language}）\n\n${fence(template)}`);
  return `${parts.join("\n\n")}\n`;
}

/** 内联图片后的 markdown 超过该字节数时，引导改用 MCP 由本地 agent 读取。 */
export const MARKDOWN_INLINE_LIMIT = 2 * 1024 * 1024;

export function markdownBytes(markdown: string): number {
  return new TextEncoder().encode(markdown).length;
}

function escapeHtml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/**
 * 与 `problemMarkdown` 平行的富文本组装器，供剪贴板 text/html flavor：
 * 结构一致，图片沿用题面 HTML（内联后的 data URL 已被 cleanHtml 放行）。
 * 数学公式保持 LaTeX 源文本——KaTeX 渲染版在接收端没有样式表会错乱。
 */
export function problemHtml(
  p: Problem,
  paper: Paper,
  options: { sourceVisible?: boolean } = {},
): string {
  const { sourceVisible = true } = options;
  const parts = [`<h1>${escapeHtml(`${p.displayLabel}. ${p.title}`)}</h1>`];
  if (sourceVisible)
    parts.push(
      `<p>来源：<a href="${escapeHtml(paper.sourceUrl)}">${escapeHtml(paper.sourceUrl)}</a></p>`,
    );
  parts.push(
    `<p>测验：${escapeHtml(paper.contest.title)} · ${kindName[p.kind]}${
      p.score != null ? ` · ${p.score} 分` : ""
    }</p>`,
  );
  if (p.timeLimit) parts.push(`<p>时间限制：${p.timeLimit} 秒 · 内存限制：${p.memoryLimit} MB</p>`);
  for (const [title, html] of [
    ["题目要求", p.description],
    ["输入说明", p.input],
    ["输出说明", p.output],
  ])
    if (html) parts.push(`<h2>${title}</h2>`, cleanHtml(html, paper.sourceUrl));
  if (p.choices.length)
    parts.push(
      `<h2>选项</h2>`,
      ...p.choices.map(
        (choice, i) =>
          `<p><strong>${String.fromCharCode(65 + i)}.</strong> ${cleanHtml(choice, paper.sourceUrl)}</p>`,
      ),
    );
  p.samples.forEach((s, i) =>
    parts.push(
      `<h3>样例 ${i + 1}</h3>`,
      `<p>输入：</p><pre>${escapeHtml(s.input)}</pre>`,
      `<p>输出：</p><pre>${escapeHtml(s.output)}</pre>`,
    ),
  );
  if (p.hint) parts.push(`<h2>提示</h2>`, cleanHtml(p.hint, paper.sourceUrl));
  for (const [language, template] of Object.entries(p.templates))
    parts.push(`<h2>代码模板（${language}）</h2>`, `<pre>${escapeHtml(template)}</pre>`);
  return parts.join("\n");
}

/** 题面里会进 markdown 的 HTML 段落；样例与代码模板是纯文本，不在此列。 */
function statementSections(p: Problem): string[] {
  return [p.description, p.input, p.output, ...p.choices, p.hint].filter(Boolean);
}

/** 收集题面各段 HTML 中的图片地址（cleanHtml 已把相对路径绝对化）。 */
export function collectStatementImageUrls(p: Problem, paper: Paper): string[] {
  const urls = new Set<string>();
  for (const html of statementSections(p)) {
    const el = document.createElement("div");
    el.innerHTML = cleanHtml(html, paper.sourceUrl);
    for (const img of el.querySelectorAll("img[src]")) urls.add(img.getAttribute("src") as string);
  }
  return [...urls];
}

/**
 * 复制题面的图片内联：对题面图片并发调用 `resolve`，返回 data URL 就
 * 替换该图的 src，返回 undefined 保留原地址（公网图、加载失败或后端拒绝）。
 */
export async function inlineStatementImages(
  p: Problem,
  paper: Paper,
  resolve: (url: string) => Promise<string | undefined>,
): Promise<Problem> {
  const resolved = new Map<string, string>();
  await Promise.all(
    collectStatementImageUrls(p, paper).map(async (url) => {
      const data = await resolve(url);
      if (data) resolved.set(url, data);
    }),
  );
  if (!resolved.size) return p;
  const replace = (html: string): string => {
    if (!html) return html;
    const el = document.createElement("div");
    el.innerHTML = cleanHtml(html, paper.sourceUrl);
    let changed = false;
    for (const img of el.querySelectorAll("img[src]")) {
      const data = resolved.get(img.getAttribute("src") as string);
      if (data) {
        img.setAttribute("src", data);
        changed = true;
      }
    }
    return changed ? el.innerHTML : html;
  };
  return {
    ...p,
    description: replace(p.description),
    input: replace(p.input),
    output: replace(p.output),
    hint: replace(p.hint),
    choices: p.choices.map(replace),
  };
}
