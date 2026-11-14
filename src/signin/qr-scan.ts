/**
 * 扫码入口：Android 走官方 tauri-plugin-barcode-scanner（原生相机界面）。
 * 桌面端不提供扫码 UI（匹配共享码由 Rust 房间完成），这里对桌面调用直接抛错。
 */
import { isAndroidPlatform } from "../os-info";

type ScanModule = typeof import("@tauri-apps/plugin-barcode-scanner");

let cachedModule: ScanModule | null = null;

async function scanner(): Promise<ScanModule> {
  if (!cachedModule) {
    cachedModule = await import("@tauri-apps/plugin-barcode-scanner");
  }
  return cachedModule;
}

/** 单次扫码：打开原生取景器，识别到二维码后返回内容；取消时抛错。 */
export async function scanQrOnce(): Promise<string> {
  if (!(await isAndroidPlatform())) {
    throw new Error("当前平台不支持扫码");
  }
  const { scan, Format } = await scanner();
  const result = await scan({ formats: [Format.QRCode] });
  return result.content;
}

/** 取消进行中的原生扫码。 */
export async function cancelScan(): Promise<void> {
  if (!(await isAndroidPlatform())) return;
  const { cancel } = await scanner();
  await cancel();
}

/**
 * 持续扫码循环（「共享签到码」模式）：每次识别到新内容就回调并立即
 * 重新打开取景器，直到调用方停止。识别到相同内容时跳过。
 */
export async function runScanLoop(
  onResult: (content: string) => void,
  isRunning: () => boolean,
): Promise<void> {
  let last = "";
  while (isRunning()) {
    try {
      const content = await scanQrOnce();
      if (!isRunning()) break;
      if (content && content !== last) {
        last = content;
        onResult(content);
      }
    } catch {
      // 用户取消取景器：结束循环。
      break;
    }
  }
}

export function stopScanLoop(): Promise<void> {
  return cancelScan();
}
