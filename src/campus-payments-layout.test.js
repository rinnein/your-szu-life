/** @vitest-environment node */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const styles = readFileSync(import.meta.dirname + "/index.css", "utf8");

describe("campus payments responsive layout contract", () => {
  it("uses two WeChat service columns on wide screens", () => {
    expect(styles).toMatch(
      /\.payments-wechat-grid\s*\{[^}]*grid-template-columns: repeat\(2, minmax\(0, 1fr\)\)/,
    );
  });
  it("stacks WeChat services in a single column on narrow screens", () => {
    expect(styles).toMatch(
      /@media \(max-width: 700px\)[\s\S]*\.payments-wechat-grid\s*\{\s*grid-template-columns: minmax\(0, 1fr\)/,
    );
  });
});
