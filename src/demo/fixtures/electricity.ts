import type {
  ElectricityConstants,
  ElectricityRecharge,
  ElectricityResult,
  ElectricitySettings,
} from "../../electricity";
import type { ElectricityCampus } from "../../electricity-location";

export const mockElectricitySettings: ElectricitySettings = {
  campusId: "demo-campus",
  areaId: "demo-area",
  buildingId: "demo-building",
  buildingName: "荔林15栋",
  roomName: "512",
};

export const mockElectricityConstants: ElectricityConstants = { yuanPerKwh: 0.58 };

export const mockElectricityOptions: ElectricityCampus[] = [
  {
    id: "demo-campus",
    name: "粤海校区",
    areas: [
      {
        id: "demo-area",
        name: "荔林生活区",
        campusId: "demo-campus",
        provider: "demo",
        buildings: [
          { id: "demo-building-13", name: "荔林13栋" },
          { id: "demo-building", name: "荔林15栋" },
        ],
      },
    ],
  },
];

function dayOffset(days: number): string {
  const date = new Date();
  date.setDate(date.getDate() + days);
  return date.toISOString().slice(0, 10);
}

export const mockElectricityRecharges: ElectricityRecharge[] = [
  {
    time: dayOffset(-20),
    kwh: 50,
    yuan: 29,
    source: "线上缴费",
    sourceKind: "channel",
  },
  {
    time: dayOffset(-8),
    kwh: 100,
    yuan: 58,
    source: "演示同学",
    sourceKind: "person",
  },
];

/** 近 14 天的读数：让电量走势图与日用量曲线有真实感。 */
const readings = Array.from({ length: 14 }, (_, index) => {
  const ago = 13 - index;
  const dailyUsed = 4 + ((index * 7) % 9) * 0.4;
  return {
    date: dayOffset(-ago),
    roomName: mockElectricitySettings.roomName,
    remaining: Math.round((36 - index * 2.4) * 10) / 10,
    totalUsed: Math.round((180 + index * dailyUsed) * 10) / 10,
    totalPurchased: Math.round((200 + index * 0.5) * 10) / 10,
    dailyUsed: Math.round(dailyUsed * 10) / 10,
    dailyPurchased: null,
  };
});

export const mockElectricityResult: ElectricityResult = {
  campusId: mockElectricitySettings.campusId,
  areaId: mockElectricitySettings.areaId,
  buildingName: mockElectricitySettings.buildingName,
  roomName: mockElectricitySettings.roomName,
  beginDate: dayOffset(-13),
  endDate: dayOffset(0),
  readings,
  currentRemaining: readings[readings.length - 1]?.remaining ?? null,
  summary: {
    totalUsed: 96.2,
    dailyAverage: 6.9,
    validDays: 14,
    predictionDailyAverage: 6.9,
    estimatedDaysLeft: 5,
    observedDate: dayOffset(0),
    expired: false,
    warningReason: null,
  },
  recharges: mockElectricityRecharges,
  rechargeError: null,
  cache: { fromCache: false, updatedAtMs: Date.now(), refreshError: null },
};
