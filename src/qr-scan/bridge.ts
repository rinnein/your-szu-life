/**
 * 扫码取景器的命令式桥：`scanQrOnce` / `runScanLoop` 只负责发出请求，
 * 浮层实例由 `QrScannerHost`（挂在 AppBridges）统一渲染，整个 WebView 最多一个。
 *
 * 与 OJ 图片查看浮层同一套模式：请求对象自带结算回调，宿主用
 * `useSyncExternalStore` 订阅。关闭路径（叉号、Escape、Android 返回键）都由
 * material dialog 的 close 统一落到 `closeScan`，因此这里只维护一份状态。
 */

export type QrScanMode = "once" | "continuous";

export type QrScanErrorCode =
  | "cancelled"
  | "unsupported"
  | "busy"
  | "permission"
  | "no-camera"
  | "in-use"
  | "failed";

export class QrScanError extends Error {
  readonly code: QrScanErrorCode;

  constructor(code: QrScanErrorCode, message: string) {
    super(message);
    this.name = "QrScanError";
    this.code = code;
  }
}

/** 用户主动取消取景器是正常路径，调用方不应提示失败。 */
export function isScanCancelled(cause: unknown): boolean {
  return cause instanceof QrScanError && cause.code === "cancelled";
}

export interface ScanRequest {
  mode: QrScanMode;
  /** 连续模式：每识别到一个新的二维码内容调用一次（同一内容在会话内去重）。 */
  onCode: (content: string) => void;
  /** 连续模式：用户主动关闭取景器（叉号 / 返回 / Escape / 停止共享）。 */
  onUserClose?: () => void;
  /** 单次模式：识别成功。 */
  resolve: (content: string) => void;
  /** 单次模式：取消或无法开始。 */
  reject: (error: QrScanError) => void;
  /** 会话结束（任意原因）时调用，供连续模式结束 await。 */
  onClosed: () => void;
}

let current: ScanRequest | undefined;
const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of listeners) listener();
}

export function subscribeScan(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** useSyncExternalStore 快照：对象引用在两次请求之间保持稳定。 */
export function getScanSnapshot(): ScanRequest | undefined {
  return current;
}

export function openScan(request: ScanRequest): void {
  if (current) throw new QrScanError("busy", "已有正在进行的扫码");
  if (listeners.size === 0) throw new QrScanError("failed", "扫码组件尚未就绪");
  current = request;
  emit();
}

/** 单次模式识别成功：先 resolve 再关闭，避免宿主卸载后回调丢失。 */
export function settleScan(content: string): void {
  const request = current;
  if (!request || request.mode !== "once") return;
  current = undefined;
  emit();
  request.resolve(content);
  request.onClosed();
}

/**
 * 关闭当前会话。`userInitiated` 表示关闭来自用户（叉号 / 返回 / Escape /
 * 停止共享）；effect 清理里的静默停止不触发连续模式的 onUserClose。
 */
export function closeScan(options: { userInitiated?: boolean } = {}): void {
  const request = current;
  if (!request) return;
  current = undefined;
  emit();
  if (request.mode === "once") {
    request.reject(new QrScanError("cancelled", "已取消扫码"));
  } else if (options.userInitiated) {
    request.onUserClose?.();
  }
  request.onClosed();
}
