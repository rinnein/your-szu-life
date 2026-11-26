import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { Button, Dialog, LoadingIndicator } from "material/react";
import { Banner } from "../ui";
import { openPermissionSettings } from "../permissions";
import { useAndroid } from "../use-android";
import { closeScan, getScanSnapshot, settleScan, subscribeScan, type ScanRequest } from "./bridge";
import {
  cameraErrorMessage,
  currentCameraId,
  listCameras,
  openCameraStream,
  stopMediaStream,
  type CameraFailure,
  type CameraOption,
} from "./camera";
import "./qr-scan.css";

/**
 * 全屏扫码取景器。
 *
 * 用 `m-dialog size="fullscreen"` 而不是自绘浮层：material 的对话框在 top
 * layer 里，天然盖在配对/签到对话框之上；叉号、Escape 与 Android 返回键都走
 * 它自己的 close，`src/android-back.ts` 无需新增选择器。相机由 qr-scanner
 * 管理解码（优先系统 BarcodeDetector，否则内联 Worker 里的 jsQR），相机流
 * 由这里自己获取，以便默认后置摄像头并支持多摄像头切换。
 */
function QrScannerDialog({ request }: { request: ScanRequest }) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const scannerRef = useRef<import("qr-scanner").default | null>(null);
  const disposedRef = useRef(false);
  const [attempt, setAttempt] = useState(0);
  const [phase, setPhase] = useState<"starting" | "scanning" | "failed">("starting");
  const [failure, setFailure] = useState<CameraFailure | null>(null);
  const [recognized, setRecognized] = useState(0);
  const [cameras, setCameras] = useState<CameraOption[]>([]);
  const [activeCameraId, setActiveCameraId] = useState("");
  const [switching, setSwitching] = useState(false);
  const lastRef = useRef("");
  const platform = useAndroid();
  const continuous = request.mode === "continuous";

  const refreshCameras = useCallback(async (stream: MediaStream | null) => {
    const options = await listCameras();
    const current = currentCameraId(stream);
    setCameras(options);
    setActiveCameraId(current || options[0]?.id || "");
  }, []);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    let disposed = false;
    disposedRef.current = false;
    let scanner: import("qr-scanner").default | undefined;
    setPhase("starting");
    setFailure(null);
    setCameras([]);
    setActiveCameraId("");

    void (async () => {
      try {
        const { default: QrScanner } = await import("qr-scanner");
        if (disposed) return;
        scanner = new QrScanner(
          video,
          (result) => {
            if (disposed) return;
            const content = result.data;
            if (!content || content === lastRef.current) return;
            lastRef.current = content;
            if (!continuous) {
              settleScan(content);
              return;
            }
            setRecognized((count) => count + 1);
            request.onCode(content);
          },
          {
            maxScansPerSecond: 10,
            returnDetailedScanResult: true,
            onDecodeError: () => undefined,
          },
        );
        scannerRef.current = scanner;
        const stream = await openCameraStream();
        if (disposed) {
          stopMediaStream(stream);
          return;
        }
        video.srcObject = stream;
        await scanner.start();
        if (disposed) return;
        setPhase("scanning");
        void refreshCameras(stream);
      } catch (cause) {
        if (disposed) return;
        stopMediaStream(video.srcObject as MediaStream | null);
        video.srcObject = null;
        setFailure(cameraErrorMessage(cause));
        setPhase("failed");
        // 默认摄像头打不开时仍枚举一次，让用户能换一个摄像头再试。
        if (scanner) {
          scannerRef.current = scanner;
          void refreshCameras(null);
        } else {
          scannerRef.current = null;
        }
      }
    })();

    return () => {
      disposed = true;
      disposedRef.current = true;
      scannerRef.current = null;
      scanner?.destroy();
      scanner = undefined;
      // qr-scanner 的 destroy 已经停了流；这里再兜一次，避免异常路径漏掉轨道。
      stopMediaStream(video.srcObject as MediaStream | null);
      video.srcObject = null;
    };
  }, [attempt, continuous, refreshCameras, request]);

  /** 按枚举顺序循环切换摄像头；新流打不开时回退到原摄像头。 */
  const switchCamera = async () => {
    const scanner = scannerRef.current;
    const video = videoRef.current;
    if (!scanner || !video || switching || cameras.length < 2) return;
    const index = cameras.findIndex((camera) => camera.id === activeCameraId);
    const next = cameras[(index + 1) % cameras.length];
    if (!next) return;
    const previousId = activeCameraId;
    setSwitching(true);
    setFailure(null);
    try {
      // Android 上同一时刻通常只能打开一个摄像头，必须先停掉旧的再开新的。
      await scanner.pause(true);
      const stream = await openCameraStream(next.id);
      if (disposedRef.current) {
        stopMediaStream(stream);
        return;
      }
      video.srcObject = stream;
      await scanner.start();
      if (disposedRef.current) return;
      setPhase("scanning");
      setActiveCameraId(next.id);
      void refreshCameras(stream);
    } catch (cause) {
      if (disposedRef.current) return;
      try {
        const fallback = await openCameraStream(previousId || undefined);
        if (disposedRef.current) {
          stopMediaStream(fallback);
          return;
        }
        video.srcObject = fallback;
        await scanner.start();
        setPhase("scanning");
        void refreshCameras(fallback);
      } catch {
        // 切换失败也把游标推到这一台，下一次点击会继续尝试下一个摄像头。
        setActiveCameraId(next.id);
        setFailure(cameraErrorMessage(cause));
        setPhase("failed");
      }
    } finally {
      setSwitching(false);
    }
  };

  return (
    <Dialog
      open
      size="fullscreen"
      className="qr-scan-dialog"
      headline={continuous ? "持续共享签到码" : "扫描二维码"}
      subtitle="将二维码放入取景框"
      actions={
        cameras.length > 1 || continuous ? (
          <>
            {cameras.length > 1 && (
              <Button
                variant="text"
                disabled={switching || phase === "starting"}
                onClick={() => void switchCamera()}
              >
                切换摄像头
              </Button>
            )}
            {continuous && (
              <Button variant="tonal" onClick={() => closeScan({ userInitiated: true })}>
                停止共享
              </Button>
            )}
          </>
        ) : undefined
      }
      onClose={() => closeScan({ userInitiated: true })}
    >
      <div className="qr-scan-viewport">
        <video ref={videoRef} className="qr-scan-video" muted playsInline />
        {phase === "starting" && (
          <div className="qr-scan-status" aria-live="polite">
            <LoadingIndicator />
            <p>正在打开摄像头…</p>
          </div>
        )}
        {phase === "failed" && failure && (
          <div className="qr-scan-status">
            <Banner status="danger">
              <Banner.Title>无法打开摄像头</Banner.Title>
              <Banner.Description>{failure.message}</Banner.Description>
            </Banner>
            <div className="flex flex-wrap justify-center gap-2">
              <Button variant="filled" onClick={() => setAttempt((value) => value + 1)}>
                重试
              </Button>
              {failure.permission && platform === "android" && (
                <Button
                  variant="tonal"
                  onClick={() => void openPermissionSettings("app").catch(() => undefined)}
                >
                  打开系统设置
                </Button>
              )}
            </div>
          </div>
        )}
        {phase === "scanning" && (
          <div className="qr-scan-hint" aria-live="polite">
            {continuous && recognized > 0
              ? `已识别 ${recognized} 个二维码，继续对准下一份`
              : "对准二维码，识别后会自动处理"}
          </div>
        )}
      </div>
    </Dialog>
  );
}

export function QrScannerHost() {
  const request = useSyncExternalStore(subscribeScan, getScanSnapshot, () => undefined);
  if (typeof document === "undefined" || !request) return null;
  return createPortal(<QrScannerDialog request={request} />, document.body);
}
