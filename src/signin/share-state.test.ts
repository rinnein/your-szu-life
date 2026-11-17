import { describe, expect, it } from "vite-plus/test";
import { shareReducer, shareStageFor, type ShareAction, type ShareState } from "./share-state";

const idle: ShareState = { stage: "idle" };

function reduce(state: ShareState, ...actions: ShareAction[]): ShareState {
  return actions.reduce(shareReducer, state);
}

describe("shareReducer", () => {
  it("idle → scanning → idle（扫码是本地相机状态，不依赖房间）", () => {
    expect(reduce(idle, { type: "scanStarted" })).toEqual({ stage: "scanning" });
    expect(reduce({ stage: "scanning" }, { type: "scanFinished" })).toEqual(idle);
  });

  it("只有 idle 可以开始扫码", () => {
    expect(reduce({ stage: "matching", courseId: 1 }, { type: "scanStarted" })).toEqual({
      stage: "matching",
      courseId: 1,
    });
  });

  it("roomJoined 在 idle 时进入匹配（含后端按课表窗口自动建房）", () => {
    expect(reduce(idle, { type: "roomJoined", courseId: 7 })).toEqual({
      stage: "matching",
      courseId: 7,
    });
    expect(
      reduce({ stage: "sharing", courseId: 7, count: 2 }, { type: "roomJoined", courseId: 7 }),
    ).toEqual({
      stage: "sharing",
      courseId: 7,
      count: 2,
    });
  });

  it("roomClosed 只回收同一课程的房间状态，不影响扫码", () => {
    expect(reduce({ stage: "matching", courseId: 7 }, { type: "roomClosed", courseId: 7 })).toEqual(
      idle,
    );
    expect(
      reduce({ stage: "sharing", courseId: 7, count: 1 }, { type: "roomClosed", courseId: 7 }),
    ).toEqual(idle);
    expect(
      reduce({ stage: "confirmShare", courseId: 7 }, { type: "roomClosed", courseId: 7 }),
    ).toEqual(idle);
    expect(reduce({ stage: "matching", courseId: 7 }, { type: "roomClosed", courseId: 8 })).toEqual(
      {
        stage: "matching",
        courseId: 7,
      },
    );
    expect(reduce({ stage: "scanning" }, { type: "roomClosed", courseId: 7 })).toEqual({
      stage: "scanning",
    });
  });

  it("共享确认页 → 开始共享（计数从 0 开始），可取消", () => {
    const confirmed = reduce(idle, { type: "sharePrompt", courseId: 7 });
    expect(confirmed).toEqual({ stage: "confirmShare", courseId: 7 });
    expect(reduce(confirmed, { type: "shareStarted", courseId: 7 })).toEqual({
      stage: "sharing",
      courseId: 7,
      count: 0,
    });
    expect(reduce(confirmed, { type: "stopped", courseId: 7 })).toEqual(idle);
  });

  it("matching 可直接升级为 sharing（join 幂等，roomJoined 事件先到不阻塞）", () => {
    expect(
      reduce({ stage: "matching", courseId: 7 }, { type: "shareStarted", courseId: 7 }),
    ).toEqual({
      stage: "sharing",
      courseId: 7,
      count: 0,
    });
  });

  it("codeShared 只在 sharing 状态计数", () => {
    expect(reduce({ stage: "sharing", courseId: 7, count: 1 }, { type: "codeShared" })).toEqual({
      stage: "sharing",
      courseId: 7,
      count: 2,
    });
    expect(reduce({ stage: "matching", courseId: 7 }, { type: "codeShared" })).toEqual({
      stage: "matching",
      courseId: 7,
    });
  });

  it("shareStopped 停掉共享循环但保留房间（回到匹配）", () => {
    expect(
      reduce({ stage: "sharing", courseId: 7, count: 3 }, { type: "shareStopped", courseId: 7 }),
    ).toEqual({
      stage: "matching",
      courseId: 7,
    });
    expect(
      reduce({ stage: "sharing", courseId: 7, count: 3 }, { type: "shareStopped", courseId: 8 }),
    ).toEqual({
      stage: "sharing",
      courseId: 7,
      count: 3,
    });
  });

  it("stopped 只作用于同一课程", () => {
    expect(reduce({ stage: "matching", courseId: 7 }, { type: "stopped", courseId: 8 })).toEqual({
      stage: "matching",
      courseId: 7,
    });
    expect(
      reduce({ stage: "sharing", courseId: 7, count: 1 }, { type: "stopped", courseId: 7 }),
    ).toEqual(idle);
  });
});

describe("shareStageFor", () => {
  it("其它课程的房间状态在本课程视角呈现为 idle", () => {
    expect(shareStageFor({ stage: "matching", courseId: 8 }, 7)).toEqual({
      stage: "idle",
      sharedCount: 0,
    });
    expect(shareStageFor({ stage: "sharing", courseId: 7, count: 2 }, 7)).toEqual({
      stage: "sharing",
      sharedCount: 2,
    });
    expect(shareStageFor({ stage: "scanning" }, 7)).toEqual({ stage: "scanning", sharedCount: 0 });
    expect(shareStageFor(idle, 7)).toEqual({ stage: "idle", sharedCount: 0 });
  });
});
