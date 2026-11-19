import { invoke } from "@tauri-apps/api/core";

export type ElectricitySettings = {
  campusId: string;
  areaId: string;
  buildingId: string;
  buildingName: string;
  roomName: string;
};
export type ElectricityReading = {
  date: string;
  observedAt: string | null;
  roomName: string;
  remaining: number | null;
  totalUsed: number | null;
  totalPurchased: number | null;
  dailyUsed: number | null;
  dailyPurchased: number | null;
};
export type ElectricitySummary = {
  totalUsed: number | null;
  /** 最近 7 天覆盖加权的日均。 */
  weeklyDailyAverage: number | null;
  /** 最近 31 天覆盖加权的日均。 */
  monthlyDailyAverage: number | null;
  /** 最终估计：上面两个可用值的均值，余额校正与预计天数都用它。 */
  predictionDailyAverage: number | null;
  /** 校正后的有效余额（读数余额 + 充值记录 − 观测后估算用电）。 */
  estimatedRemaining: number | null;
  rechargeCreditKwh: number;
  elapsedUsedKwh: number;
  estimatedDaysLeft: number | null;
  observedDate: string | null;
  expired: boolean;
  warningReason: string | null;
};
export type ElectricityRecharge = {
  time: string;
  kwh: number | null;
  yuan: number | null;
  source: string | null;
  sourceKind: "person" | "channel";
};
export type ElectricityResult = {
  campusId: string;
  areaId: string;
  buildingName: string;
  roomName: string;
  beginDate: string;
  endDate: string;
  readings: ElectricityReading[];
  currentRemaining: number | null;
  observedRemaining: number | null;
  observedAtMs: number | null;
  rechargeCreditKwh: number;
  elapsedUsedKwh: number;
  summary: ElectricitySummary;
  recharges: ElectricityRecharge[];
  rechargeError: string | null;
  cache?: { fromCache: boolean; updatedAtMs: number | null; refreshError: string | null } | null;
};
export type ElectricityConstants = { yuanPerKwh: number };
export function getElectricityRoomFloor(roomName: string): number | null {
  const normalized = roomName.trim();
  if (!/^\d{3,4}$/.test(normalized)) return null;
  return Number(normalized.slice(0, -2));
}
/** 只有确实拿到读数或余额时才算查询成功，避免把无效宿舍落库。 */
export function hasElectricityData(
  result: Pick<ElectricityResult, "currentRemaining" | "readings">,
): boolean {
  return result.currentRemaining != null || result.readings.length > 0;
}
/** Backend-owned tariff; no duplicate frontend rate or silently assumed fallback. */
let constantsRequest: Promise<ElectricityConstants> | undefined;
export function getElectricityConstants() {
  return (constantsRequest ??= invoke<ElectricityConstants>("get_electricity_constants").catch(
    (error) => {
      constantsRequest = undefined;
      throw error;
    },
  ));
}
export function formatElectricity(value: number | null | undefined) {
  return value == null || !Number.isFinite(value) ? "—" : value.toFixed(2);
}
