/**
 * 签到/打卡模块的前端类型镜像（对应 szu-signin / szu-lms 的 Rust 模型）。
 * 序列化命名沿用后端：嵌套条目 snake_case，顶层快照 camelCase。
 */

/** 内置教学地点（signin-buildings.json，GCJ-02）。 */
export interface Building {
  id: number;
  name: string;
  name_en: string;
  campus: string;
  latitude_gcj02: number;
  longitude_gcj02: number;
  radius_m: number;
  status: string;
}

export function isProvisional(building: Building): boolean {
  return building.status === "provisional";
}

/** 课表中的一个课次。 */
export interface TimetableSession {
  date: string;
  course_name: string;
  building_name?: string;
  building_name_en?: string;
  classroom?: string;
  start_ms: number;
  end_ms: number;
  semester?: string;
}

export interface TimetableCache {
  fetched_at_ms: number;
  semester?: string;
  sessions: TimetableSession[];
}

/** 畅课待签项（szu-lms rollcall::RollcallItem）。 */
export interface RollcallItem {
  rollcall_id: number;
  is_number: boolean;
  is_radar: boolean;
  course_id: number;
  course_title: string;
  student_rollcall_id: number;
  kind?: string;
  status?: string;
}

export type RollcallMethod = "radar" | "number" | "qr" | "unknown";

export function rollcallMethod(item: RollcallItem): RollcallMethod {
  if (item.is_radar) return "radar";
  if (item.is_number) return "number";
  switch (item.kind) {
    case "qr":
    case "qr_rollcall":
    case "qrcode":
      return "qr";
    case "number":
    case "number_rollcall":
      return "number";
    case "radar":
    case "gps":
      return "radar";
    default:
      return "unknown";
  }
}

export function needAnswer(item: RollcallItem): boolean {
  return item.status !== "on_call_fine";
}

/** 雷达默认点解析结果（camelCase，Rust ResolvedPoint）。 */
export interface ResolvedPoint {
  latitude: number;
  longitude: number;
  label: string;
  source: "last_success" | "timetable" | "fallback";
  fenceRadiusM?: number;
}

/** SZU 上次照片视图（camelCase，Rust LastPhotoView）。 */
export interface LastPhotoView {
  hasPhoto: boolean;
  thumbnail?: string;
}

/** 签到码共享房间事件（Rust QrEvent，tag=type content=data camelCase）。 */
export type QrEvent =
  | { type: "roomJoined"; data: { courseId: number } }
  | { type: "roomClosed"; data: { courseId: number } }
  | { type: "matched"; data: { courseId: number; ok: boolean; message: string } }
  | { type: "shared"; data: { courseId: number; ok: boolean; message: string } }
  | { type: "error"; data: { message: string } };

export function localDateKey(date: Date = new Date()): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function formatSessionTime(startMs: number, endMs: number): string {
  const start = new Date(startMs);
  const end = new Date(endMs);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${pad(start.getHours())}:${pad(start.getMinutes())} - ${pad(end.getHours())}:${pad(end.getMinutes())}`;
}
