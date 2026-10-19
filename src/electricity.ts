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
  roomName: string;
  remaining: number | null;
  totalUsed: number | null;
  totalPurchased: number | null;
  dailyUsed: number | null;
  dailyPurchased: number | null;
};
export type ElectricitySummary = {
  totalUsed: number | null;
  dailyAverage: number | null;
  validDays: number;
  predictionDailyAverage: number | null;
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
