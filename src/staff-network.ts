export type StaffLoginMode = "macAuth" | "appAuto";
export type StaffCredentialMode = "currentAccount" | "custom";
export type StaffSettings = {
  revision: number;
  mode: StaffLoginMode;
  enabled: boolean;
  credentialMode: StaffCredentialMode;
  customConfigured: boolean;
  needsApply: boolean;
  appliedMode?: StaffLoginMode | null;
};
export type StaffSettingsInput = {
  mode: StaffLoginMode;
  enabled: boolean;
  credentialMode: StaffCredentialMode;
  campusCardNumber?: string;
  password?: string;
};
export type StaffStatus = {
  state: "disabled" | "monitoring" | "probing" | "loggingIn" | "online" | "paused" | "error";
  consecutiveFailures: number;
  nextProbeAtMs?: number | null;
  message?: string | null;
};
export type StaffApplyResult = {
  outcome: "applied" | "requiresConfirmation" | "alreadyOnline" | "failed" | "unknown";
  message: string;
  confirmationToken?: string | null;
};
export const initialStaffSettings: StaffSettings = {
  revision: 0,
  mode: "macAuth",
  enabled: false,
  credentialMode: "currentAccount",
  customConfigured: false,
  needsApply: true,
  appliedMode: null,
};
export const initialStaffStatus: StaffStatus = { state: "disabled", consecutiveFailures: 0 };
export function staffModeText(mode?: StaffLoginMode | null) {
  return mode === "macAuth"
    ? "无感认证（绑定MAC地址）"
    : mode === "appAuto"
      ? "软件自动登录（不绑定MAC地址）"
      : "暂无";
}
