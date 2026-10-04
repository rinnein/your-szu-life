/** @vitest-environment jsdom */
import { describe, expect, it } from "vite-plus/test";
import { cleanHtml, hasContestDescription, fence, htmlMarkdown, problemMarkdown } from "./content";
import type { Paper, Problem } from "./types";
const p: Problem = {
  id: "1",
  displayLabel: "A",
  kind: 0,
  title: "求和",
  description: "<p>计算 <strong>a + b</strong></p><p>$x^2$</p>",
  input: "<p>两个整数</p>",
  output: "<p>它们的和</p>",
  hint: "<p>注意溢出</p>",
  timeLimit: 1,
  memoryLimit: 128,
  samples: [{ input: "  1 2\n", output: "3\n" }],
  choices: [],
  multiple: false,
  blanks: 0,
  languages: ["cpp"],
  templates: { cpp: "int main() {\n}\n" },
  ioMode: "stdio",
};
const paper = { sourceUrl: "http://oj.test/#/contest/12/", contest: { title: "练习" } } as Paper;
describe("OJ problem content", () => {
  it("removes active HTML and unsafe URLs", () => {
    const html = cleanHtml(
      '<script>alert(1)</script><img src="/image.png" onerror="bad()"><a href="javascript:bad()">x</a><iframe src="https://bad.test"></iframe>',
      paper.sourceUrl,
    );
    expect(html).not.toMatch(/script|onerror|iframe/);
    expect(html).toContain("http://oj.test/image.png");
  });
  it("exports all public statement sections", () => {
    const value = problemMarkdown(p, paper);
    for (const text of [
      "求和",
      "题目要求",
      "输入说明",
      "输出说明",
      "样例 1",
      "提示",
      "注意溢出",
      "代码模板",
      "时间限制",
    ])
      expect(value).toContain(text);
    expect(value).toContain("  1 2\n");
  });
  it("preserves formulas and tables", () => {
    const value = htmlMarkdown(
      "<p>$x^2$</p><table><thead><tr><th>A</th><th>B</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table>",
      paper.sourceUrl,
    );
    expect(value).toContain("$x^2$");
    expect(value).toContain("| A | B |");
  });
  it("extracts existing latex annotations", () =>
    expect(htmlMarkdown('<span data-latex="x^2">rendered</span>', paper.sourceUrl)).toContain(
      "$x^2$",
    ));
  it("preserves preformatted whitespace", () =>
    expect(htmlMarkdown("<pre>  a\n\n b\t</pre>", paper.sourceUrl)).toContain("  a\n\n b\t"));
  it("uses safe fences for code containing backticks", () =>
    expect(fence("```\nvalue")).toMatch(/^````text/));
  it("keeps images as absolute Markdown links", () =>
    expect(htmlMarkdown('<img src="/media/a.png" alt="diagram">', paper.sourceUrl)).toContain(
      "![diagram](http://oj.test/media/a.png)",
    ));
  it("includes non-programming choices", () =>
    expect(problemMarkdown({ ...p, kind: 1, choices: ["<b>甲</b>", "乙"] }, paper)).toContain(
      "A. **甲**",
    ));
});

it("hides the bare contest deadline placeholder while preserving useful descriptions", () => {
  for (const value of ["", "截止时间：", "<p>截止时间:</p>", "<p> 截止时间&nbsp;：<br></p>"])
    expect(hasContestDescription(value, paper.sourceUrl)).toBe(false);
  for (const value of [
    "截止时间：2026-10-06 12:00",
    "请在截止时间之前提交。",
    '<p>截止时间：</p><img src="/deadline.png">',
    '<a href="/notice">截止时间：</a>',
  ])
    expect(hasContestDescription(value, paper.sourceUrl)).toBe(true);
});
