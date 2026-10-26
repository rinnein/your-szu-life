/** @vitest-environment node */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vite-plus/test";

const styles = readFileSync(import.meta.dirname + "/index.css", "utf8");

// 微信服务的布局契约：充值页与校园卡页各自只渲染一块 WeChatServiceCard，
// 卡内竖向排布，操作按钮在宽屏靠左、窄屏拉满。
// （旧的两列 `.payments-wechat-grid` 网格随旧版页面一起消失。）
describe("campus payments responsive layout contract", () => {
  it("stacks the WeChat service content vertically", () => {
    expect(styles).toMatch(/\.payments-wechat-content\s*\{[^}]*flex-direction: column/);
  });
  it("keeps the action button left-aligned on wide screens", () => {
    expect(styles).toMatch(/\.payments-wechat-content > button\s*\{\s*align-self: flex-start/);
  });
  it("stretches the action button on narrow screens", () => {
    expect(styles).toMatch(
      /@media \(max-width: 700px\)[\s\S]*\.payments-wechat-content > button\s*\{\s*align-self: stretch/,
    );
  });
});
