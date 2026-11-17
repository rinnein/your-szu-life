/**
 * 二维码共享房间的状态与动作 hook（供畅课面板持有）。
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
import { runScanLoop, scanQrOnce, stopScanLoop } from "./qr-scan";
import { qrActionFromEvent, qrReducer, type QrState } from "./qr-state";
import type { QrEvent } from "./types";

export function useQrRoom() {
  const [state, dispatch] = useReducer(qrReducer, { stage: "idle" } as QrState);
  const pending = usePending();

  // 房间事件：joined/closed 驱动状态机；matched 是提交结果（失败不阻断后续匹配）。
  useChannelSubscription<QrEvent>("subscribe_signin_qr_events", (event) => {
    const action = qrActionFromEvent(event);
    if (action) {
      dispatch(action);
      return;
    }
    if (event.type === "matched") {
      if (event.data.ok) toast.success(event.data.message);
      else toast.warning(event.data.message);
    }
  });

  // 持续共享的扫码循环：进入 sharing 时启动，离开该状态时停掉本地循环。
  const sharingRef = useRef(false);
  useEffect(() => {
    if (state.stage !== "sharing") return;
    sharingRef.current = true;
    void runScanLoop(
      async (content) => {
        const outcome = await pending.run("share", () =>
          tauriCore.invoke<{ shared: boolean }>("signin_qr_room_share", { raw: content }),
        );
        if (outcome.ok) {
          if (outcome.value.shared) dispatch({ type: "codeShared" });
        } else if (!outcome.skipped) {
          toast.warning("共享失败", { description: errorMessage(outcome.error) });
        }
      },
      () => sharingRef.current,
    );
    return () => {
      sharingRef.current = false;
      void stopScanLoop().catch(() => undefined);
    };
  }, [state.stage, pending]);

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
      // 平台不支持是真错误；用户取消取景器不提示。
      if (String(cause).includes("不支持扫码")) {
        toast.danger(errorMessage(cause));
      }
    } finally {
      dispatch({ type: "scanFinished" });
    }
  }, []);

  const startMatching = useCallback(
    async (courseId: number, rollcallId: number) => {
      const outcome = await pending.run("join", () =>
        tauriCore.invoke("signin_qr_room_join", { courseId, rollcallId }),
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
      const outcome = await pending.run("leave", () => tauriCore.invoke("signin_qr_room_leave"));
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
        tauriCore.invoke("signin_qr_room_join", { courseId }),
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

export type QrRoom = ReturnType<typeof useQrRoom>;
