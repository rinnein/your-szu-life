/**
 * 畅课签到码共享区（纯展示）：状态来自 useShareRoom 的房间状态机，动作由
 * 宿主（LmsPanel）传入。平台未知时不渲染操作区，避免 Android 首帧闪现
 * 桌面分支；匹配状态在对话框关闭后仍由面板持有并显示。
 */
import { Button } from "material/react";
import { shareStageFor, type ShareState } from "./share-state";
import { useAndroid } from "../use-android";

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
  const platform = useAndroid();
  const { stage, sharedCount } = shareStageFor(state, courseId);

  if (platform === "unknown") return null;

  if (platform === "desktop") {
    return (
      <div className="flex flex-col gap-2" data-testid="share-section-desktop">
        {stage === "matching" ? (
          <>
            <p className="text-sm text-muted">共享签到码匹配中…</p>
            <Button variant="tonal" onClick={() => void actions.stop(courseId)}>
              停止匹配
            </Button>
          </>
        ) : (
          <>
            <Button
              variant="filled"
              onClick={() => void actions.startMatching(courseId, rollcallId)}
            >
              开始匹配共享签到码
            </Button>
            <p className="text-xs text-muted">
              桌面端无法扫描二维码；请在同一课程的其他设备上扫码或提交数字码，收到共享后本机会自动提交。
            </p>
          </>
        )}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2" data-testid="share-section-android">
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
          <Button
            variant="filled"
            disabled={stage === "scanning"}
            onClick={() => void actions.scanOnce(rollcallId)}
          >
            {stage === "scanning" ? "等待扫码…" : "扫码签到"}
          </Button>
          <Button variant="tonal" onClick={() => void actions.startMatching(courseId, rollcallId)}>
            等待他人共享
          </Button>
          <Button variant="tonal" onClick={() => actions.promptShare(courseId)}>
            持续共享签到码
          </Button>
        </>
      )}
    </div>
  );
}
