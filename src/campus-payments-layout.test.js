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

describe("material component narrow screen layout contract", () => {
  it("strips automatic minimum inline size from material custom elements", () => {
    expect(styles).toMatch(/m-tabs,\s*m-text-field,\s*m-select,\s*m-datepicker/);
    expect(styles).toMatch(/m-tabs,[\s\S]*?min-inline-size:\s*0/);
    expect(styles).toMatch(/m-card\s*\{[^}]*min-inline-size:\s*0/);
  });

  it("permits text wrapping on card headlines and subheads", () => {
    expect(styles).toMatch(
      /m-card\s+\[slot="subhead"\],\s*m-card\s+\[slot="headline"\]\s*\{[^}]*white-space:\s*normal/,
    );
  });

  it("permits top app bar actions to wrap without pushing off-screen", () => {
    expect(styles).toMatch(/m-top-app-bar::part\(top-app-bar\)[\s\S]*?flex-wrap:\s*wrap/);
    expect(styles).toMatch(/m-top-app-bar::part\(headline\)[\s\S]*?min-inline-size:\s*0/);
    expect(styles).toMatch(/m-top-app-bar::part\(trailing\)[\s\S]*?min-inline-size:\s*0/);
  });

  it("ensures banners, disclosures, and data tables can shrink within grid tracks", () => {
    expect(styles).toMatch(/\.banner\s*\{[^}]*min-inline-size:\s*0/);
    expect(styles).toMatch(/\.disclosure\s*\{[^}]*min-inline-size:\s*0/);
    expect(styles).toMatch(/\.data-table-scroll\s*\{[^}]*min-inline-size:\s*0/);
  });
});
