import { useCallback, useEffect } from "react";
import { useAsyncLoad, type Async } from "../async-state";
import { isAndroidPlatform } from "../os-info";

/**
 * 摄像头可用性检测。
 *
 * 判定规则刻意偏保守：
 * - 没有 `mediaDevices.getUserMedia`（旧内核 / 非安全上下文）→ 不可用；
 * - Android 手机一律可用：未授权时 WebView 的 `enumerateDevices` 可能返回空
 *   列表或缺少 videoinput，不能据此把入口藏掉；
 * - 枚举成功、列表非空但没有 videoinput → 不可用；
 * - 枚举失败或列表为空 → 无法判断，按「可能有摄像头」处理，点开后再报具体错误。
 */
export async function detectCamera(): Promise<boolean> {
  const media = typeof navigator === "undefined" ? undefined : navigator.mediaDevices;
  if (!media || typeof media.getUserMedia !== "function") return false;
  if (await isAndroidPlatform()) return true;
  try {
    const devices = await media.enumerateDevices();
    if (devices.length === 0) return true;
    return devices.some((device) => device.kind === "videoinput");
  } catch {
    return true;
  }
}

export function useCameraAvailability(): Async<boolean> {
  const load = useCallback(() => detectCamera(), []);
  const { state, reload } = useAsyncLoad(load);
  useEffect(() => {
    const media = typeof navigator === "undefined" ? undefined : navigator.mediaDevices;
    if (!media?.addEventListener) return;
    const onChange = () => void reload();
    media.addEventListener("devicechange", onChange);
    return () => media.removeEventListener("devicechange", onChange);
  }, [reload]);
  return state;
}

export interface CameraOption {
  id: string;
  label: string;
}

/**
 * 自己获取相机流，而不是把相机选择交给 qr-scanner 的 `_getCameraStream`。
 *
 * qr-scanner 默认先用 `facingMode: { exact: "environment" }` + `width.min`
 * 连续试三次；桌面摄像头没有 facingMode，WebKitGTK 会为这几次失败各打一条
 * 「no device was found amongst N devices」。这里统一用 `ideal` 约束：
 * Android 默认后置（没有后置时浏览器自动回落），桌面交给系统默认摄像头；
 * 切换摄像头时按 deviceId 精确打开。
 */
export async function openCameraStream(deviceId?: string): Promise<MediaStream> {
  const media = typeof navigator === "undefined" ? undefined : navigator.mediaDevices;
  if (!media || typeof media.getUserMedia !== "function") {
    throw new Error("当前环境不支持调用摄像头");
  }
  const video: MediaTrackConstraints = {
    width: { ideal: 1280 },
    height: { ideal: 720 },
  };
  if (deviceId) {
    video.deviceId = { exact: deviceId };
  } else if (await isAndroidPlatform()) {
    video.facingMode = { ideal: "environment" };
  }
  return media.getUserMedia({ audio: false, video });
}

export function stopMediaStream(stream: MediaStream | null | undefined): void {
  if (!stream || typeof stream.getTracks !== "function") return;
  for (const track of stream.getTracks()) track.stop();
}

/** 枚举摄像头；调用方应在拿到相机权限（流已启动）后调用，标签才完整。 */
export async function listCameras(): Promise<CameraOption[]> {
  const media = typeof navigator === "undefined" ? undefined : navigator.mediaDevices;
  if (!media?.enumerateDevices) return [];
  try {
    const devices = await media.enumerateDevices();
    return devices
      .filter((device) => device.kind === "videoinput")
      .map((device, index) => ({
        id: device.deviceId,
        label: device.label || `摄像头 ${index + 1}`,
      }));
  } catch {
    return [];
  }
}

export function currentCameraId(stream: MediaStream | null | undefined): string {
  const tracks = stream?.getVideoTracks?.();
  const track = tracks?.[0];
  return track?.getSettings?.().deviceId ?? "";
}

export interface CameraFailure {
  message: string;
  /** 是否属于权限被拒绝，界面可据此提供「打开系统设置」。 */
  permission: boolean;
}

/** 把 getUserMedia / qr-scanner 的失败映射成中文说明。 */
export function cameraErrorMessage(cause: unknown): CameraFailure {
  const name = cause instanceof DOMException ? cause.name : "";
  switch (name) {
    case "NotAllowedError":
    case "SecurityError":
      return {
        message: "相机权限被拒绝。请在系统设置中允许本应用使用相机后重试。",
        permission: true,
      };
    case "NotFoundError":
    case "OverconstrainedError":
      return { message: "未检测到可用的摄像头。", permission: false };
    case "NotReadableError":
    case "AbortError":
      return { message: "摄像头被其他应用占用，请关闭后重试。", permission: false };
    default:
      break;
  }
  const text = cause instanceof Error ? cause.message : typeof cause === "string" ? cause : "";
  if (/permission|denied|not allowed/i.test(text)) {
    return {
      message: "相机权限被拒绝。请在系统设置中允许本应用使用相机后重试。",
      permission: true,
    };
  }
  if (/no camera|camera not found|requested device not found/i.test(text)) {
    return { message: "未检测到可用的摄像头。", permission: false };
  }
  return { message: "无法打开摄像头，请重试。", permission: false };
}
