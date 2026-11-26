/**
 * 畅课签到码共享区（纯展示）：状态来自 useShareRoom 的房间状态机，动作由
 * 宿主（LmsPanel）传入。扫码入口只在检测到摄像头时出现；没有摄像头时仍可
 * 「等待他人共享」——被动匹配与自动提交不依赖本机相机。
 */
import { Button } from "material/react";
import { shareStageFor, type ShareState } from "./share-state";
import { useCameraAvailability } from "../qr-scan";

export interface ShareActions {
  scanOnce: (rollcallId: number) => Promise<void>;
  startMatching: (courseId: number, rollcallId: number) => Promise<void>;
  stop: (courseId: number) => Promise<void>;
  promptShare: (courseId: number) => void;
  cancelSharePrompt: (courseId: number) => void;
  startSharing: (courseId: number) => Promise<void>;
}

export function ShareSection({
  rollcallId,
  courseId,
  state,
  actions,
}: {
  rollcallId: number;
  courseId: number;
  state: ShareState;
  actions: ShareActions;
}) {
  const camera = useCameraAvailability();
  const canScan = camera.status === "ready" && camera.data;
  const noCamera = camera.status === "ready" && !camera.data;
  const { stage, sharedCount } = shareStageFor(state, courseId);

  return (
    <div className="flex flex-col gap-2" data-testid="share-section">
      {stage === "sharing" ? (
        <>
          <p className="text-sm font-medium" aria-live="polite">
            已成功共享 <span data-testid="shared-count">{sharedCount}</span> 份签到码
          </p>
          <Button variant="tonal" onClick={() => void actions.stop(courseId)}>
            停止共享
          </Button>
        </>
      ) : stage === "confirmShare" ? (
        <>
          <p className="text-sm text-muted">
            该功能用于持续上传新的签到码：二维码 15
            秒内有效，数字码在本次点名结束前有效。您可以自行决定什么时候开始与结束。
          </p>
          <Button variant="filled" onClick={() => void actions.startSharing(courseId)}>
            开始
          </Button>
          <Button variant="text" onClick={() => actions.cancelSharePrompt(courseId)}>
            取消
          </Button>
        </>
      ) : stage === "matching" ? (
        <>
          <p className="text-sm text-muted">共享签到码匹配中，收到后会自动提交…</p>
          <Button variant="tonal" onClick={() => void actions.stop(courseId)}>
            停止匹配
          </Button>
        </>
      ) : (
        <>
          {canScan && (
            <Button
              variant="filled"
              disabled={stage === "scanning"}
              onClick={() => void actions.scanOnce(rollcallId)}
            >
              {stage === "scanning" ? "等待扫码…" : "扫码签到"}
            </Button>
          )}
          <Button variant="tonal" onClick={() => void actions.startMatching(courseId, rollcallId)}>
            等待他人共享
          </Button>
          {canScan && (
            <Button variant="tonal" onClick={() => actions.promptShare(courseId)}>
              持续共享签到码
            </Button>
          )}
          {noCamera && (
            <p className="text-xs text-muted">
              未检测到摄像头；可让同一课程的其他设备扫码，本机收到共享后会自动提交。
            </p>
          )}
        </>
      )}
    </div>
  );
}
