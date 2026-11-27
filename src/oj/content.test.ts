/** @vitest-environment jsdom */
import { describe, expect, it } from "vite-plus/test";
import {
  cleanHtml,
  hasContestDescription,
  fence,
  htmlMarkdown,
  problemMarkdown,
  problemHtml,
  collectStatementImageUrls,
  inlineStatementImages,
  markdownBytes,
  MARKDOWN_INLINE_LIMIT,
} from "./content";
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
  it("keeps image data URIs on img elements only", () => {
    const html = cleanHtml(
      '<img src="data:image/png;base64,AAAA" alt="a"><a href="data:text/html,bad">x</a>',
      paper.sourceUrl,
    );
    expect(html).toContain('src="data:image/png;base64,AAAA"');
    expect(html).not.toContain('href="data:"');
  });
  it("keeps every image data URI subtype on img elements", () => {
    // 题面里常见编辑器导出的 `image/jpg` 等非标准子类型，不能只认 png/jpeg/gif/webp。
    const html = cleanHtml(
      [
        '<img src="data:image/jpg;base64,AAAA" alt="jpg">',
        '<img src="data:image/bmp;base64,BBBB" alt="bmp">',
        '<img src="data:image/svg+xml;base64,CCCC" alt="svg">',
      ].join(""),
      paper.sourceUrl,
    );
    for (const value of [
      "data:image/jpg;base64,AAAA",
      "data:image/bmp;base64,BBBB",
      "data:image/svg+xml;base64,CCCC",
    ])
      expect(html).toContain(`src="${value}"`);
  });
  it("still drops non-image data URIs", () => {
    const html = cleanHtml('<img src="data:text/html,<b>x</b>" alt="x">', paper.sourceUrl);
    expect(html).not.toContain("src=");
  });
  it("omits the source line on request", () => {
    expect(problemMarkdown(p, paper)).toContain("来源：http://oj.test/#/contest/12/");
    expect(problemMarkdown(p, paper, { sourceVisible: false })).not.toContain("来源：");
  });
  it("collects absolute image urls from statement sections", () => {
    const withImage = { ...p, description: '<img src="/media/a.png" alt="a">' };
    expect(collectStatementImageUrls(withImage, paper)).toEqual(["http://oj.test/media/a.png"]);
  });
  it("skips already-inline image data URIs when collecting", () => {
    // data URI 不需要巡检校园网可达性，也不必经 IPC 送去后端取图。
    const withImage = {
      ...p,
      description: '<img src="data:image/jpg;base64,AAAA" alt="a"><img src="/media/b.png" alt="b">',
    };
    expect(collectStatementImageUrls(withImage, paper)).toEqual(["http://oj.test/media/b.png"]);
  });
  it("inlines resolved images and keeps the rest", async () => {
    const withImage: Problem = {
      ...p,
      description: '<p>看图</p><img src="http://oj.test/media/a.png" alt="图A">',
      choices: ['<img src="/media/b.png" alt="图B">'],
    };
    const resolved = await inlineStatementImages(withImage, paper, async (url) =>
      url === "http://oj.test/media/a.png" ? "data:image/png;base64,AAAA" : undefined,
    );
    expect(resolved.description).toContain('src="data:image/png;base64,AAAA"');
    // 未命中的图片保留原样，绝对化交给 problemMarkdown 内部的 cleanHtml。
    expect(resolved.choices[0]).toBe('<img src="/media/b.png" alt="图B">');
    const markdown = problemMarkdown(resolved, paper, { sourceVisible: false });
    expect(markdown).toContain("![图A](data:image/png;base64,AAAA)");
    expect(markdown).toContain("![图B](http://oj.test/media/b.png)");
  });
  it("measures markdown bytes against the inline limit", () => {
    expect(markdownBytes("abc")).toBe(3);
    expect(markdownBytes("中")).toBe(3);
    expect(MARKDOWN_INLINE_LIMIT).toBe(2 * 1024 * 1024);
  });
  it("assembles rich text mirroring the markdown structure", () => {
    const html = problemHtml(p, paper);
    expect(html).toContain("<h1>A. 求和</h1>");
    expect(html).toContain("来源：<a href=");
    expect(html).toContain("<h2>题目要求</h2>");
    expect(html).toContain("<pre><code>  1 2\n</code></pre>");
    expect(html).toContain("<h2>代码模板（cpp）</h2>");
    expect(html).toContain("<pre><code>int main() {\n}\n</code></pre>");
  });
  it("embeds data-url images and omits the source line on request", () => {
    const withImage: Problem = {
      ...p,
      description: '<img src="data:image/png;base64,AAAA" alt="图A">',
    };
    const html = problemHtml(withImage, paper, { sourceVisible: false });
    expect(html).toContain('<img src="data:image/png;base64,AAAA"');
    expect(html).not.toContain("来源：");
  });
  it("escapes sample text and lists choices", () => {
    const tricky: Problem = {
      ...p,
      samples: [{ input: "<script>alert(1)</script>", output: "3" }],
      choices: ["<b>甲</b>"],
    };
    const html = problemHtml(tricky, paper, { sourceVisible: false });
    expect(html).toContain("<pre><code>&lt;script&gt;alert(1)&lt;/script&gt;</code></pre>");
    expect(html).toContain("<strong>A.</strong>");
    expect(html).toContain("<b>甲</b>");
  });
});

/** 从复制出的片段里把样例内容抠回来，用于判断空白是否逐字节保真。 */
function sampleBlocks(html: string): string[] {
  const doc = new DOMParser().parseFromString(html, "text/html");
  return [...doc.querySelectorAll("pre")].map((pre) => pre.textContent ?? "");
}

/**
 * 抠出围栏代码块内容。fence() 保证正文恒以恰好一个换行收尾、随后是行首关闭围栏，
 * 因此按"下一个行首反引号行"切分即可（不存在嵌套围栏：题目样例里没有反引号行）。
 */
function fencedBlocks(markdown: string): string[] {
  const blocks: string[] = [];
  const open = "```text\n";
  let from = 0;
  for (;;) {
    const start = markdown.indexOf(open, from);
    if (start < 0) return blocks;
    const bodyStart = start + open.length;
    const close = /\n`{3,}(?=\n|$)/g;
    close.lastIndex = bodyStart;
    const match = close.exec(markdown);
    if (!match) return blocks;
    blocks.push(markdown.slice(bodyStart, match.index + 1));
    from = match.index + match[0].length - 1;
  }
}

describe("OJ sample copy fidelity", () => {
  const cases: [name: string, input: string][] = [
    ["行末空格", "1 2 \n3\n"],
    ["末行行末空格且无换行", "1 2\n3  "],
    ["末行行末空格且有换行", "1 2\n3  \n"],
    ["前导空行", "\n1 2\n"],
    ["前导空格", "   1 2\n"],
    ["制表符", "a\tb\n"],
    ["空行结尾", "1 2\n\n"],
    ["CRLF", "1 2\r\n3\r\n"],
    ["空样例", ""],
  ];

  it("keeps every sample byte in the Markdown code block", () => {
    for (const [name, input] of cases) {
      const markdown = problemMarkdown(
        { ...p, templates: {}, samples: [{ input, output: "o" }] },
        paper,
        { sourceVisible: false },
      );
      // 代码块内容恒以恰好一个换行收尾（围栏必须独占一行），其余字节原样保留：
      // 行末空格、前导空行、制表符、CRLF 都在。
      const expected = input.endsWith("\n") ? input : `${input}\n`;
      expect(fencedBlocks(markdown), name).toEqual([expected, "o\n"]);
    }
  });

  it("keeps every sample byte in the rich-text code block", () => {
    for (const [name, input] of cases) {
      const html = problemHtml({ ...p, templates: {}, samples: [{ input, output: "o" }] }, paper, {
        sourceVisible: false,
      });
      // 浏览器解析 HTML 时会吞掉 `<pre>` 开始标签后紧跟的第一个换行，因此样例必须
      // 包在 `<code>` 里（标签隔开），否则以空行开头的样例粘贴出来会少一行。
      // CRLF 同时归一到 LF。
      expect(sampleBlocks(html)[0], name).toBe(input.replace(/\r\n?/g, "\n"));
    }
  });

  it("wraps samples in a code element so chat clients treat them as code", () => {
    const html = problemHtml(
      { ...p, templates: {}, samples: [{ input: "1 2\n", output: "3\n" }] },
      paper,
      { sourceVisible: false },
    );
    const doc = new DOMParser().parseFromString(html, "text/html");
    const blocks = [...doc.querySelectorAll("pre")];
    expect(blocks).toHaveLength(2);
    for (const block of blocks) expect(block.querySelector("code")).not.toBeNull();
  });

  it("keeps CRLF verbatim in Markdown but normalizes it in rich text", () => {
    const sample = { input: "1\r\n2\r\n", output: "o" };
    const markdown = problemMarkdown({ ...p, templates: {}, samples: [sample] }, paper, {
      sourceVisible: false,
    });
    // Markdown 一侧与单样例复制按钮一致，逐字节原样（不解码 CRLF）。
    expect(fencedBlocks(markdown)[0]).toBe("1\r\n2\r\n");
    // 浏览器解析 <pre> 时本来就把 CRLF 读成 LF，这里显式对齐解析后的结果。
    const html = problemHtml({ ...p, templates: {}, samples: [sample] }, paper, {
      sourceVisible: false,
    });
    expect(sampleBlocks(html)[0]).toBe("1\n2\n");
  });
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
