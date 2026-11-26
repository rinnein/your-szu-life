/**
 * 通用扫码入口：Web `getUserMedia` + qr-scanner，Android / Linux / Windows
 * 共用同一条链路。取景器是 material 全屏对话框，返回键与 Escape 复用现有
 * 浮层关闭逻辑，不再依赖原生扫码插件。
 */
import { closeScan, openScan, QrScanError } from "./bridge";

export { isScanCancelled, QrScanError } from "./bridge";
export { useCameraAvailability } from "./camera";

function hasMediaDevices(): boolean {
  return (
    typeof navigator !== "undefined" && typeof navigator.mediaDevices?.getUserMedia === "function"
  );
}

/** 单次扫码：打开取景器，识别到二维码后返回内容；用户取消时抛 cancelled。 */
export function scanQrOnce(): Promise<string> {
  if (!hasMediaDevices()) {
    return Promise.reject(new QrScanError("unsupported", "当前环境不支持调用摄像头"));
  }
  return new Promise<string>((resolve, reject) => {
    try {
      openScan({
        mode: "once",
        onCode: () => undefined,
        resolve,
        reject,
        onClosed: () => undefined,
      });
    } catch (cause) {
      reject(cause instanceof QrScanError ? cause : new QrScanError("failed", "无法开始扫码"));
    }
  });
}

/**
 * 连续扫码会话（如「持续共享签到码」）：取景器保持打开，每识别到一个新的
 * 内容就回调；同一内容在会话内只回调一次。调用 `stopScanLoop` 静默结束，
 * 用户点叉号/返回/停止共享时先触发 `onUserClose` 再结束。
 */
export async function runScanLoop(
  onResult: (content: string) => void,
  isRunning: () => boolean,
  onUserClose?: () => void,
): Promise<void> {
  if (!hasMediaDevices()) throw new QrScanError("unsupported", "当前环境不支持调用摄像头");
  await new Promise<void>((resolve, reject) => {
    try {
      openScan({
        mode: "continuous",
        onCode: (content) => {
          if (isRunning()) onResult(content);
        },
        onUserClose,
        resolve: () => undefined,
        reject,
        onClosed: resolve,
      });
    } catch (cause) {
      reject(cause instanceof QrScanError ? cause : new QrScanError("failed", "无法开始扫码"));
    }
  });
}

/** 静默结束连续扫码（effect 清理用，不触发 onUserClose）。 */
export function stopScanLoop(): void {
  closeScan();
}
