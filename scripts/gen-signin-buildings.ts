/**
 * 生成根目录 `signin-buildings.json`：签到/打卡模块内置的教学地点列表。
 *
 * 数据源 `docs/qw-attendance/buildings.json`（qw.szu.edu.cn 考勤系统楼栋接口的
 * 规范化抓取，gitignored）中剔除 `status === "placeholder"` 的占位坐标行
 * （28 条全部共用同一个假坐标 22.5447,113.948，不能当真实位置使用），保留
 * listed（正式坐标）与 provisional（服务端标注待实地校准，半径放宽 300m）。
 *
 * 运行：vpx ./scripts/gen-signin-buildings.ts
 */
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

interface SourceBuilding {
  id: number;
  name: string;
  name_en: string;
  campus: string;
  address?: string;
  description?: string;
  latitude_gcj02: number;
  longitude_gcj02: number;
  latitude_wgs84?: number;
  longitude_wgs84?: number;
  radius_m: number;
  status: string;
}

interface SourceFile {
  meta: Record<string, unknown>;
  buildings: SourceBuilding[];
}

const root = resolve(import.meta.dirname, "..");
const source = JSON.parse(
  readFileSync(resolve(root, "docs/qw-attendance/buildings.json"), "utf8"),
) as SourceFile;

const buildings = source.buildings
  .filter((b) => b.status !== "placeholder")
  .map(({ id, name, name_en, campus, latitude_gcj02, longitude_gcj02, radius_m, status }) => ({
    id,
    name,
    name_en,
    campus,
    latitude_gcj02,
    longitude_gcj02,
    radius_m,
    status,
  }));

const output = {
  meta: {
    source: source.meta.source,
    fetched_at: source.meta.fetched_at,
    coordinate_system: "GCJ-02",
    // 坐标必须按 GCJ-02 解释并提交（服务端围栏判定与高德底图一致）；
    // 浏览器 geolocation 的 WGS-84 读数需先转换为 GCJ-02。
    note: "签到/打卡模块内置地点列表：剔除 placeholder 占位坐标后的 qw.szu.edu.cn 楼栋围栏数据",
    status_counts: {
      listed: buildings.filter((b) => b.status === "listed").length,
      provisional: buildings.filter((b) => b.status === "provisional").length,
    },
    count: buildings.length,
  },
  buildings,
};

writeFileSync(resolve(root, "signin-buildings.json"), `${JSON.stringify(output, null, 2)}\n`);
console.log(`signin-buildings.json: ${buildings.length} buildings`);
