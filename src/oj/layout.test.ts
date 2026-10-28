import { describe, expect, it } from "vite-plus/test";
import { OJ_COMPACT_MAX_BLOCK, OJ_DOCK_MIN_INLINE, OJ_SPLIT_MIN_INLINE, ojLayout } from "./layout";

describe("ojLayout", () => {
  it("容器够宽就停靠侧栏，与竖直空间无关", () => {
    expect(ojLayout(true, false)).toBe("docked");
    expect(ojLayout(true, true)).toBe("docked");
  });

  it("容器不够宽时按竖直空间在横条与抽屉之间选", () => {
    expect(ojLayout(false, false)).toBe("strip");
    expect(ojLayout(false, true)).toBe("sheet");
  });

  it("停靠阈值扣掉侧栏与间距后仍留得下可读的题面", () => {
    expect(OJ_DOCK_MIN_INLINE - 240 - 32).toBeGreaterThanOrEqual(600);
    expect(OJ_SPLIT_MIN_INLINE).toBeGreaterThan(240);
    expect(OJ_COMPACT_MAX_BLOCK).toBeGreaterThan(0);
  });
});
