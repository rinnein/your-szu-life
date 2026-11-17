/**
 * 签到地点选择的纯模型：选择联合、提交参数翻译与默认项文案。
 *
 * 与后端 signin_submit_lms_radar / signin_submit_szu 的参数语义对齐
 * （custom_is_wgs84）：浏览器 geolocation 读数是 WGS-84，传 true 由 Rust
 * 转 GCJ-02；「默认」与「内置楼栋」不携带自定义坐标参数。
 */
import type { ResolvedPoint } from "./types";

export type PointSelection =
  | { kind: "default" }
  | { kind: "building"; buildingId: number; label: string }
  | {
      kind: "custom";
      latitude: number;
      longitude: number;
      isWgs84: boolean;
      label: string;
    };

/** 把选择翻译成 signin_submit_* 的可选参数；默认选择不带任何坐标参数。 */
export function selectionArgs(selection: PointSelection): Record<string, unknown> {
  switch (selection.kind) {
    case "default":
      return {};
    case "building":
      return { buildingId: selection.buildingId };
    case "custom":
      return {
        customLatitude: selection.latitude,
        customLongitude: selection.longitude,
        customIsWgs84: selection.isWgs84,
      };
  }
}

const SOURCE_LABEL: Record<NonNullable<ResolvedPoint["source"]>, string> = {
  last_success: "上次成功定位",
  timetable: "课表位置",
  fallback: "兜底地点",
};

/**
 * 默认项文案：显示解析出的地点与来源；解析中/失败时说明当前状态
 * （失败仍可提交，后端会按默认解析链兜底）。
 */
export function defaultPointLabel(
  resolved: ResolvedPoint | null,
  status: "idle" | "loading" | "ready" | "error",
): string {
  if (status === "loading" || status === "idle") return "默认地点解析中…";
  if (status === "error" || !resolved) return "默认（上次成功定位 → 课表 → 兜底）";
  const source = SOURCE_LABEL[resolved.source] ?? "默认解析";
  return `${resolved.label}（默认 · ${source}）`;
}
