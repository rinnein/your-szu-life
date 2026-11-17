/**
 * 二维码共享房间的界面状态机（纯 reducer，事件驱动）。
 *
 * 后端按课表窗口自动建立/回收房间，并通过 Channel 推送 roomJoined /
 * roomClosed；界面必须跟随这两个事件，否则会出现「房间已被回收但界面
 * 仍在匹配」。房间状态按 course_id 归属：状态属于其它课程时，本课程的
 * 视角呈现为 idle（见 qrStageFor）。
 */
import type { QrEvent } from "./types";

export type QrState =
  | { stage: "idle" }
  | { stage: "scanning" }
  | { stage: "matching"; courseId: number }
  | { stage: "confirmShare"; courseId: number }
  | { stage: "sharing"; courseId: number; count: number };

export type QrAction =
  | { type: "scanStarted" }
  | { type: "scanFinished" }
  | { type: "roomJoined"; courseId: number }
  | { type: "roomClosed"; courseId: number }
  | { type: "sharePrompt"; courseId: number }
  | { type: "shareStarted"; courseId: number }
  | { type: "codeShared" }
  /** 用户停止（离开房间），或关闭确认页。 */
  | { type: "stopped"; courseId: number }
  /** 持续共享的扫码循环停止，但房间仍在（被动匹配继续）。 */
  | { type: "shareStopped"; courseId: number };

type RoomStage = Extract<QrState, { courseId: number }>;

function isRoomStage(state: QrState, courseId?: number): state is RoomStage {
  if (state.stage !== "matching" && state.stage !== "confirmShare" && state.stage !== "sharing") {
    return false;
  }
  return courseId === undefined || state.courseId === courseId;
}

export function qrReducer(state: QrState, action: QrAction): QrState {
  switch (action.type) {
    case "scanStarted":
      return state.stage === "idle" ? { stage: "scanning" } : state;
    case "scanFinished":
      return state.stage === "scanning" ? { stage: "idle" } : state;
    case "roomJoined":
      // 自动房间（探测循环按课表窗口建立）也会推送 roomJoined：空闲时进入匹配。
      return state.stage === "idle" ? { stage: "matching", courseId: action.courseId } : state;
    case "roomClosed":
      return isRoomStage(state, action.courseId) ? { stage: "idle" } : state;
    case "sharePrompt":
      return state.stage === "idle" ? { stage: "confirmShare", courseId: action.courseId } : state;
    case "shareStarted":
      // 开始共享前都会先 join（幂等）；matching 直接升级，避免与 roomJoined 事件竞态。
      if (state.stage === "idle" || state.stage === "confirmShare") {
        return state.stage === "confirmShare" && state.courseId !== action.courseId
          ? state
          : { stage: "sharing", courseId: action.courseId, count: 0 };
      }
      if (state.stage === "matching" && state.courseId === action.courseId) {
        return { stage: "sharing", courseId: action.courseId, count: 0 };
      }
      return state;
    case "codeShared":
      return state.stage === "sharing" ? { ...state, count: state.count + 1 } : state;
    case "stopped":
      return isRoomStage(state, action.courseId) ? { stage: "idle" } : state;
    case "shareStopped":
      return state.stage === "sharing" && state.courseId === action.courseId
        ? { stage: "matching", courseId: state.courseId }
        : state;
  }
}

/** 房间归属其它课程时，本课程视角按 idle 呈现。 */
export function qrStageFor(
  state: QrState,
  courseId: number,
): { stage: QrState["stage"]; sharedCount: number } {
  if (state.stage === "idle" || state.stage === "scanning") {
    return { stage: state.stage, sharedCount: 0 };
  }
  if (state.courseId !== courseId) {
    return { stage: "idle", sharedCount: 0 };
  }
  return {
    stage: state.stage,
    sharedCount: state.stage === "sharing" ? state.count : 0,
  };
}

/** 从后端 QrEvent 提取界面关心的动作；matched/shared 等结果事件由调用方处理。 */
export function qrActionFromEvent(event: QrEvent): QrAction | null {
  switch (event.type) {
    case "roomJoined":
      return { type: "roomJoined", courseId: event.data.course_id };
    case "roomClosed":
      return { type: "roomClosed", courseId: event.data.course_id };
    default:
      return null;
  }
}
