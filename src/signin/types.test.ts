import { describe, expect, it } from "vite-plus/test";
import { needAnswer, rollcallMethod, rollcallStatusLabel, type RollcallItem } from "./types";
import { defaultPointLabel, selectionArgs } from "./point-selection";

const item = (over: Partial<RollcallItem>): RollcallItem => ({
  rollcall_id: 1,
  is_number: false,
  is_radar: false,
  course_id: 1,
  course_title: "课程",
  student_rollcall_id: 1,
  ...over,
});

describe("rollcallMethod", () => {
  it('读取线上键 type（Rust 侧 rename = "type"，不是 kind）', () => {
    expect(rollcallMethod(item({ type: "qr" }))).toBe("qr");
    expect(rollcallMethod(item({ type: "qr_rollcall" }))).toBe("qr");
    expect(rollcallMethod(item({ type: "number_rollcall" }))).toBe("number");
    expect(rollcallMethod(item({ type: "gps" }))).toBe("radar");
    expect(rollcallMethod(item({}))).toBe("unknown");
    expect(rollcallMethod(item({ type: "mystery" }))).toBe("unknown");
  });

  it("is_radar / is_number 优先于 type（与畅课 H5 跳转优先级一致）", () => {
    expect(rollcallMethod(item({ is_radar: true, type: "qr" }))).toBe("radar");
    expect(rollcallMethod(item({ is_number: true, type: "qr" }))).toBe("number");
  });
});

describe("rollcallStatusLabel", () => {
  it("映射已知状态，未知与缺省兜底为「进行中」", () => {
    expect(rollcallStatusLabel("on_call")).toBe("进行中");
    expect(rollcallStatusLabel("on_call_fine")).toBe("已签到");
    expect(rollcallStatusLabel("absent")).toBe("缺席");
    expect(rollcallStatusLabel("some_new_status")).toBe("进行中");
    expect(rollcallStatusLabel(undefined)).toBe("进行中");
  });
});

describe("needAnswer", () => {
  it("只有 on_call_fine 视为已完成", () => {
    expect(needAnswer(item({ status: "on_call" }))).toBe(true);
    expect(needAnswer(item({ status: undefined }))).toBe(true);
    expect(needAnswer(item({ status: "on_call_fine" }))).toBe(false);
  });
});

describe("selectionArgs", () => {
  it("默认选择不携带任何坐标参数（走后端解析链）", () => {
    expect(selectionArgs({ kind: "default" })).toEqual({});
  });

  it("内置楼栋 → buildingId", () => {
    expect(selectionArgs({ kind: "building", buildingId: 79, label: "致理楼" })).toEqual({
      buildingId: 79,
    });
  });

  it("自定义坐标保留 isWgs84 标志（浏览器读数 true，上次成功点 false）", () => {
    expect(
      selectionArgs({
        kind: "custom",
        latitude: 22.5,
        longitude: 113.9,
        isWgs84: true,
        label: "x",
      }),
    ).toEqual({ customLatitude: 22.5, customLongitude: 113.9, customIsWgs84: true });
    expect(
      selectionArgs({
        kind: "custom",
        latitude: 22.5,
        longitude: 113.9,
        isWgs84: false,
        label: "x",
      }),
    ).toEqual({ customLatitude: 22.5, customLongitude: 113.9, customIsWgs84: false });
  });
});

describe("defaultPointLabel", () => {
  const point = { latitude: 22.5, longitude: 113.9, label: "致理楼", source: "fallback" as const };

  it("就绪时显示地点名与来源", () => {
    expect(defaultPointLabel(point, "ready")).toBe("致理楼（默认 · 兜底地点）");
    expect(defaultPointLabel({ ...point, source: "last_success" }, "ready")).toBe(
      "致理楼（默认 · 上次成功定位）",
    );
  });

  it("解析中与失败时说明状态（失败仍可提交，后端兜底）", () => {
    expect(defaultPointLabel(null, "loading")).toBe("默认地点解析中…");
    expect(defaultPointLabel(null, "idle")).toBe("默认地点解析中…");
    expect(defaultPointLabel(null, "error")).toBe("默认（上次成功定位 → 课表 → 兜底）");
  });
});
