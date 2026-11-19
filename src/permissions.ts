import { invoke } from "@tauri-apps/api/core";

/**
 * 权限项 id：与 Kotlin `PermissionBridge`、Rust `permissions::PermissionId` 一致，
 * 新增一项必须同步三处。
 */
export type PermissionKey =
  | "notifications"
  | "promotedNotifications"
  | "location"
  | "nearbyWifi"
  | "camera"
  | "installPackages"
  | "exactAlarm";

export type PermissionStatus = "granted" | "denied" | "unsupported" | "systemDisabled" | "unknown";

export type PermissionState = {
  id: PermissionKey;
  status: PermissionStatus;
  /** 是否还能通过系统弹窗申请；安装未知应用、精确闹钟只能跳系统设置。 */
  canRequest: boolean;
  note?: string;
};

export type PermissionSettingsTarget =
  | "app"
  | "notifications"
  | "location"
  | "installPackages"
  | "exactAlarm";

export function getPermissionStates(): Promise<PermissionState[]> {
  return invoke<PermissionState[]>("get_permission_states");
}

export function requestPermissions(keys: PermissionKey[]): Promise<PermissionState[]> {
  return invoke<PermissionState[]>("request_permissions", { keys });
}

export function openPermissionSettings(target: PermissionSettingsTarget): Promise<void> {
  return invoke<void>("open_permission_settings", { target });
}
