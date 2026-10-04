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
export function problemMarkdown(p: Problem, paper: Paper): string {
  const parts = [
    `# ${p.displayLabel}. ${p.title}`,
    `来源：${paper.sourceUrl}`,
    `测验：${paper.contest.title} · ${kindName[p.kind]}${p.score != null ? ` · ${p.score} 分` : ""}`,
  ];
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
