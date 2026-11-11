import type { StaffSettings, StaffStatus } from "../../staff-network";
import { mockProfile } from "./auth";

// CampusNetworkPage.tsx 的 NetworkOverview / Status 是组件内局部类型，无法导入，
// 这里按其字段结构（CampusNetworkPage.tsx:63-82）平行定义。
export type MockNetworkOverview = {
  state: "noNetwork" | "unknown" | "notLoggedIn" | "connected";
  connected?: boolean;
  region: "none" | "staff" | "dormitory";
  ssid?: string | null;
  portalReachable: boolean;
  staffHostReachable: boolean;
  portalSession?: {
    campusCardNumber?: string | null;
    onlineUsername?: string | null;
    onlineDuration?: number | null;
    onlineFlow?: number | null;
    loginIp?: string | null;
    loginStartedAt?: string | null;
    loginExpiresAt?: string | null;
    charset?: string | null;
  } | null;
  staffSession?: unknown;
};

export type MockDormStatus = {
  state: "disabled" | "monitoring" | "probing" | "loggingIn" | "online" | "paused" | "error";
  ssid?: string | null;
  connected: boolean;
  portalReachable: boolean;
  consecutiveFailures: number;
  nextProbeAtMs?: number | null;
  lastResult?: { ok: boolean; alreadyOnline: boolean; message?: string | null } | null;
};

export const mockNetworkOverview: MockNetworkOverview = {
  state: "connected",
  connected: true,
  region: "dormitory",
  ssid: "SZU-Dormitory",
  portalReachable: true,
  staffHostReachable: false,
  portalSession: {
    campusCardNumber: "988801234567",
    onlineUsername: mockProfile.studentId,
    onlineDuration: 5021,
    onlineFlow: 3_612_214_784,
    loginIp: "10.20.30.40",
    loginStartedAt: new Date(Date.now() - 5_021_000).toISOString(),
    loginExpiresAt: null,
    charset: "UTF-8",
  },
  staffSession: null,
};

export const mockDormStatus: MockDormStatus = {
  state: "online",
  ssid: "SZU-Dormitory",
  connected: true,
  portalReachable: true,
  consecutiveFailures: 0,
  nextProbeAtMs: Date.now() + 300_000,
  lastResult: { ok: true, alreadyOnline: false, message: "在线" },
};

export const mockDormSettings = {
  enabled: true,
  credentialMode: "currentAccount",
  customConfigured: false,
  customCampusCardNumber: null,
};

export const mockStaffSettings: StaffSettings = {
  revision: 1,
  mode: "macAuth",
  enabled: false,
  credentialMode: "currentAccount",
  customConfigured: false,
  customCampusCardNumber: null,
  needsApply: false,
  appliedMode: "macAuth",
};

export const mockStaffStatus: StaffStatus = {
  state: "disabled",
  consecutiveFailures: 0,
  nextProbeAtMs: null,
  message: null,
};
