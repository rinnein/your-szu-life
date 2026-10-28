/** @vitest-environment node */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vite-plus/test";

const styles = readFileSync(import.meta.dirname + "/oj.css", "utf8");

// OJ 的布局契约：题目选择器的档位由 TS 侧的实测宽度决定（src/oj/layout.ts），
// CSS 只消费 data-* 属性。这里守住「不再回到视口媒体查询 + 互相独立的显隐条件」
// 那套写法 —— 那正是窄屏下两套题目选择器同时出现的成因。
describe("oj css layout contract", () => {
  it("题目选择器的两档窄屏形态由类名表达", () => {
    expect(styles).toMatch(/\.oj-switcher-strip\s*\{/);
    expect(styles).toMatch(/\.oj-switcher-compact\s*\{/);
  });

  it("判定胶囊所在的 icon 槽放宽，否则会压住标题", () => {
    expect(styles).toMatch(/\.oj-problem-drawer::part\(item-icon\)/);
    expect(styles).toMatch(/\.oj-problem-tabs::part\(icon\)/);
    expect(styles).toMatch(/\.oj-switcher-compact-trigger::part\(icon\)/);
    expect(styles).toMatch(/\.oj-problem-drawer::part\(item-icon\)[\s\S]{0,200}?width:\s*auto/);
  });

  it("常开抽屉停掉宽度动画，模态抽屉保留进出动画", () => {
    expect(styles).toMatch(
      /\.oj-problem-drawer:not\(\[modal\]\)::part\(drawer\)\s*\{[^}]*transition:\s*none/,
    );
  });

  it("停靠抽屉与题面并排由属性驱动", () => {
    expect(styles).toMatch(
      /\[data-oj-layout="docked"\]\s+\.oj-workspace\s*\{[^}]*grid-template-columns:\s*240px/,
    );
    expect(styles).toMatch(
      /\[data-oj-layout="docked"\]\s+\.oj-problem-drawer\s*\{[^}]*position:\s*sticky/,
    );
    expect(styles).toMatch(/\[data-oj-reading="split"\]\s+\.oj-reading\s*\{/);
    expect(styles).toMatch(/\[data-oj-reading="split"\]\s+\.oj-desktop-answer\s*\{/);
  });

  it("样例并排用基于题面宽度的容器查询", () => {
    expect(styles).toMatch(
      /\.oj-statement-sections\s*\{[^}]*container:\s*oj-statement\s*\/\s*inline-size/,
    );
    expect(styles).toMatch(
      /@container oj-statement \(min-width: 640px\)\s*\{[\s\S]*?\.oj-sample\s*\{[^}]*grid-template-columns:\s*1fr 1fr/,
    );
  });

  it("OJ 布局不再依赖视口尺寸的媒体查询", () => {
    expect(styles).not.toMatch(/@media[^{]*\b(?:min|max)-(?:width|height)\b/);
  });

  it("手写题目目录样式已让位给 Drawer", () => {
    expect(styles).not.toMatch(/\.oj-directory-item/);
    expect(styles).not.toMatch(/\.oj-directory[\s{,]/);
    expect(styles).not.toMatch(/\.oj-directory\s*\{[^}]*display:\s*flex/);
  });
});
