/**
 * 签到码共享房间（数字码 + 二维码）的状态与动作 hook（供畅课面板持有）。
 *
 * 状态提升到面板层：对话框关闭后「匹配中」仍在（被动匹配由后端继续，
 * 结果经 matched 事件 toast），列表行尾也能显示匹配状态。关闭承载共享的
 * 对话框时由宿主调用 `dialogClosed` 停掉扫码循环（房间保留）。
 */
import { useCallback, useEffect, useReducer, useRef } from "react";
import * as tauriCore from "@tauri-apps/api/core";
import { toast } from "../ui";
import { errorMessage } from "../async-state";
import { useChannelSubscription } from "../tauri-channel";
import { usePending } from "../use-pending";
import { isScanCancelled, runScanLoop, scanQrOnce, stopScanLoop } from "../qr-scan";
import { shareActionFromEvent, shareReducer, type ShareState } from "./share-state";
import type { ShareEvent } from "./types";

export function useShareRoom() {
  const [state, dispatch] = useReducer(shareReducer, { stage: "idle" } as ShareState);
  const pending = usePending();

  // 房间事件：joined/closed 驱动状态机；matched 是提交结果（失败不阻断后续匹配）。
  useChannelSubscription<ShareEvent>("subscribe_signin_share_events", (event) => {
    const action = shareActionFromEvent(event);
    if (action) {
      dispatch(action);
      return;
    }
    if (event.type === "matched") {
      if (event.data.ok) toast.success(event.data.message);
      else
        toast.warning(event.data.message, {
          description:
            event.data.kind === "number"
              ? "共享的数字码未能自动提交，请手动核对签到状态。"
              : "共享的二维码未能自动提交，请等待下一份或手动扫码。",
        });
    }
  });

  // 持续共享的扫码循环：进入 sharing 时启动，离开该状态时停掉本地循环。
  // `run` 是 usePending 里稳定的回调；不能依赖整个 pending 对象，否则每次
  // keys 变化都会重启取景器（相机会跟着闪断）。
  const shareRun = pending.run;
  const sharingRef = useRef(false);
  const sharingCourseId = state.stage === "sharing" ? state.courseId : undefined;
  useEffect(() => {
    if (sharingCourseId === undefined) return;
    const courseId = sharingCourseId;
    sharingRef.current = true;
    void runScanLoop(
      async (content) => {
        const outcome = await shareRun("share", () =>
          tauriCore.invoke<{ shared: boolean }>("signin_share_room_share_qr", { raw: content }),
        );
        if (outcome.ok) {
          if (outcome.value.shared) dispatch({ type: "codeShared" });
        } else if (!outcome.skipped) {
          toast.warning("共享失败", { description: errorMessage(outcome.error) });
        }
      },
      () => sharingRef.current,
      // 用户在取景器里关闭/停止共享：回到 matching，房间保留给被动匹配。
      () => dispatch({ type: "shareStopped", courseId }),
    ).catch((cause) => {
      if (isScanCancelled(cause)) return;
      toast.danger("无法开始扫码", { description: errorMessage(cause) });
      dispatch({ type: "shareStopped", courseId });
    });
    return () => {
      sharingRef.current = false;
      stopScanLoop();
    };
  }, [sharingCourseId, shareRun]);

  const scanOnce = useCallback(async (rollcallId: number) => {
    dispatch({ type: "scanStarted" });
    try {
      const content = await scanQrOnce();
      try {
        await tauriCore.invoke("signin_submit_lms_qr", { rollcallId, raw: content });
        toast.success("扫码签到成功");
      } catch (cause) {
        toast.danger("扫码签到失败", { description: errorMessage(cause) });
      }
    } catch (cause) {
      // 用户取消取景器是正常路径，不提示。
      if (!isScanCancelled(cause)) {
        toast.danger("无法扫码", { description: errorMessage(cause) });
      }
    } finally {
      dispatch({ type: "scanFinished" });
    }
  }, []);

  const startMatching = useCallback(
    async (courseId: number, rollcallId: number) => {
      const outcome = await pending.run("join", () =>
        tauriCore.invoke("signin_share_room_join", { courseId, rollcallId }),
      );
      if (!outcome.ok) {
        if (!outcome.skipped) {
          toast.danger("加入签到码房间失败", { description: errorMessage(outcome.error) });
        }
        return;
      }
      // join 幂等且房间可能已存在（事件不一定重发），直接进入匹配态。
      dispatch({ type: "roomJoined", courseId });
      toast.info("共享签到码匹配中，收到有效签到码后会自动提交");
    },
    [pending],
  );

  const stop = useCallback(
    async (courseId: number) => {
      const outcome = await pending.run("leave", () => tauriCore.invoke("signin_share_room_leave"));
      if (!outcome.ok) return; // leave 失败保持原状，等待房间事件回收
      dispatch({ type: "stopped", courseId });
    },
    [pending],
  );

  const promptShare = useCallback(
    (courseId: number) => dispatch({ type: "sharePrompt", courseId }),
    [],
  );

  const cancelSharePrompt = useCallback(
    (courseId: number) => dispatch({ type: "stopped", courseId }),
    [],
  );

  const startSharing = useCallback(
    async (courseId: number) => {
      const outcome = await pending.run("join", () =>
        tauriCore.invoke("signin_share_room_join", { courseId }),
      );
      if (!outcome.ok) {
        if (!outcome.skipped) {
          toast.danger("加入签到码房间失败", { description: errorMessage(outcome.error) });
        }
        return;
      }
      dispatch({ type: "roomJoined", courseId });
      dispatch({ type: "shareStarted", courseId });
    },
    [pending],
  );

  const dialogClosed = useCallback(
    (courseId: number) => dispatch({ type: "shareStopped", courseId }),
    [],
  );

  return {
    state,
    scanOnce,
    startMatching,
    stop,
    promptShare,
    cancelSharePrompt,
    startSharing,
    dialogClosed,
  };
}

export type ShareRoom = ReturnType<typeof useShareRoom>;
