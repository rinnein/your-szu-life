/**
 * 畅课二维码签到区。
 *
 * - Android：「扫码」一次性扫并直接提交（成功即共享）；「匹配共享签到码」
 *   加入房间被动匹配；「共享签到码」先确认再进入持续扫码广播给同课程设备。
 * - 桌面端：进入即加入房间匹配共享码，收到即提交，失败仅 toast 不中断。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "material/react";
import * as tauriCore from "@tauri-apps/api/core";
import { toast } from "../ui";
import { useChannelSubscription } from "../tauri-channel";
import { isAndroidPlatform } from "../os-info";
import { runScanLoop, scanQrOnce, stopScanLoop } from "./qr-scan";
import type { QrEvent } from "./types";

interface QrSectionProps {
  rollcallId: number;
  courseId: number;
}

type QrMode = "idle" | "matching" | "share-prompt" | "sharing";

export function QrSection({ rollcallId, courseId }: QrSectionProps) {
  const [android, setAndroid] = useState(false);
  const [mode, setMode] = useState<QrMode>("idle");
  const [sharedCount, setSharedCount] = useState(0);
  const [scanning, setScanning] = useState(false);
  const runningRef = useRef(false);
  const sharedCountRef = useRef(0);

  useEffect(() => {
    void isAndroidPlatform().then(setAndroid);
    return () => {
      // 卸载只停掉本地扫码循环；房间生命周期由后端探测循环按课表窗口
      // （开始/结束 ±10min）统一启停，避免窗口内误关共享。
      runningRef.current = false;
      void stopScanLoop().catch(() => undefined);
    };
  }, []);

  // 房间事件：匹配结果用 toast 呈现，失败不阻断后续匹配。
  const onEvent = useCallback((event: QrEvent) => {
    switch (event.type) {
      case "matched":
        if (event.data.ok) {
          toast.success(event.data.message);
        } else {
          toast.warning(event.data.message);
        }
        break;
      case "error":
        toast.danger(event.data.message);
        break;
      default:
        break;
    }
  }, []);
  useChannelSubscription<QrEvent>("subscribe_signin_qr_events", onEvent);

  const joinRoom = useCallback(async () => {
    try {
      await tauriCore.invoke("signin_qr_room_join", { courseId, rollcallId });
    } catch (error) {
      toast.danger("加入签到码房间失败", { description: String(error) });
      throw error;
    }
  }, [courseId, rollcallId]);

  // 扫码：识别 → 提交 → 成功后由后端顺手共享。
  const handleScanOnce = async () => {
    setScanning(true);
    try {
      const content = await scanQrOnce();
      try {
        await tauriCore.invoke("signin_submit_lms_qr", { rollcallId, raw: content });
        toast.success("扫码签到成功");
      } catch (error) {
        toast.danger("扫码签到失败", { description: String(error) });
      }
    } catch (error) {
      if (String(error).includes("不支持扫码")) {
        toast.danger(String(error));
      }
      // 用户取消扫码不提示。
    } finally {
      setScanning(false);
    }
  };

  // 匹配共享签到码：加入房间后等待他人共享（桌面同款流程）。
  const handleMatch = async () => {
    try {
      await joinRoom();
    } catch {
      return;
    }
    setMode("matching");
    toast.info("共享签到码匹配中，收到有效签到码后会自动提交");
  };

  const stopMatching = async () => {
    await tauriCore.invoke("signin_qr_room_leave").catch(() => undefined);
    setMode("idle");
  };

  // 持续共享：先展示说明与开始按钮，点击后才进入连续扫码。
  const startSharing = async () => {
    try {
      await joinRoom();
    } catch {
      return;
    }
    sharedCountRef.current = 0;
    setSharedCount(0);
    runningRef.current = true;
    setMode("sharing");
    void runScanLoop(
      async (content) => {
        try {
          const outcome = await tauriCore.invoke<{ shared: boolean }>("signin_qr_room_share", {
            raw: content,
          });
          if (outcome.shared) {
            sharedCountRef.current += 1;
            setSharedCount(sharedCountRef.current);
            toast.success(`已成功共享 ${sharedCountRef.current} 份签到码`);
          }
        } catch (error) {
          toast.warning("共享失败", { description: String(error) });
        }
      },
      () => runningRef.current,
    );
  };

  const stopSharing = async () => {
    runningRef.current = false;
    await stopScanLoop().catch(() => undefined);
    await tauriCore.invoke("signin_qr_room_leave").catch(() => undefined);
    setMode("idle");
  };

  if (!android) {
    // 桌面端：只做匹配。
    return (
      <div className="flex flex-col gap-2" data-testid="qr-section-desktop">
        {mode === "matching" ? (
          <>
            <p className="text-sm text-muted">共享签到码匹配中…</p>
            <Button variant="tonal" onClick={() => void stopMatching()}>
              停止匹配
            </Button>
          </>
        ) : (
          <>
            <Button variant="filled" onClick={() => void handleMatch()}>
              开始匹配共享签到码
            </Button>
            <p className="text-xs text-muted">
              桌面端无法扫描二维码；请在同一课程的其他设备上扫码，收到共享后本机会自动提交。
            </p>
          </>
        )}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2" data-testid="qr-section-android">
      {mode === "sharing" ? (
        <>
          <p className="text-sm font-medium">
            已成功共享 <span data-testid="shared-count">{sharedCount}</span> 份签到码
          </p>
          <Button variant="tonal" onClick={() => void stopSharing()}>
            停止共享
          </Button>
        </>
      ) : mode === "share-prompt" ? (
        <>
          <p className="text-sm text-muted">
            该功能用于持续上传新的签到码（15s 有效期），您可以自行决定什么时候开始与结束。
          </p>
          <Button variant="filled" onClick={() => void startSharing()}>
            开始
          </Button>
        </>
      ) : mode === "matching" ? (
        <>
          <p className="text-sm text-muted">共享签到码匹配中，收到后会自动提交…</p>
          <Button variant="tonal" onClick={() => void stopMatching()}>
            停止匹配
          </Button>
        </>
      ) : (
        <>
          <Button variant="filled" disabled={scanning} onClick={() => void handleScanOnce()}>
            {scanning ? "等待扫码…" : "扫码签到"}
          </Button>
          <Button variant="tonal" onClick={() => void handleMatch()}>
            匹配共享签到码
          </Button>
          <Button variant="tonal" onClick={() => setMode("share-prompt")}>
            共享签到码
          </Button>
        </>
      )}
    </div>
  );
}
