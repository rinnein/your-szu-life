import type { AppContentSnapshot } from "../../app-content/types";
import { mockTodoItems } from "./todos";

/**
 * 演示模式的公共内容快照。
 *
 * 公告与更新都来自远端仓库，无 IPC 的浏览器环境里拉不到，首页公告栏会整块
 * 空掉。这里给一份纯字面量的「已就绪」快照：不设 `pic`（避免联网取头图）、
 * 不设 `goto`（避免点开真实外链），只留标题与说明。
 */
const now = Date.now();

export const mockAppContentSnapshot: AppContentSnapshot = {
  revision: 1,
  version: "0.16.1",
  configuration: { status: "ready", message: null },
  repository: {
    fullName: "rinnein/your-szu-life",
    url: "https://github.com/rinnein/your-szu-life",
  },
  repositoryMessage: null,
  refreshing: false,
  nextAutomaticCheckAt: now + 24 * 60 * 60 * 1000,
  updateChannel: "stable",
  updateChannelFallback: false,
  announcements: {
    data: {
      announcements: [
        {
          title: "演示数据：本页内容均为模拟数据",
          id: "demo-1",
          mode: "silent",
          desc: "泥深在演示模式下不会读取任何真实账号信息，所有课程、成绩、账单与提交记录都是虚构的。",
        },
        {
          title: "新版本支持课表待签提醒",
          id: "demo-2",
          mode: "silent",
          desc: "课表现在会提前提示需要签到的课程，并在首页汇总当天的签到与截止待办。",
        },
        {
          title: "考试周的复习清单已经整理好",
          id: "demo-3",
          mode: "silent",
          desc: "把 lms 作业、UOOC 考试与 OJ 测验统一收进首页日历，再也不用逐个平台翻。",
        },
      ],
    },
    lastAttemptAt: now,
    lastSuccessAt: now,
    source: "primary",
    fromCache: false,
    error: null,
  },
  update: {
    data: null,
    lastAttemptAt: now,
    lastSuccessAt: now,
    source: "primary",
    fromCache: false,
    error: null,
  },
  updateAvailable: false,
};

/** 演示待办里最早的一条，供公告类提示在需要示例时复用。 */
export const mockAppContentEarliestDue = mockTodoItems[0]?.dueAtMs ?? now;

/**
 * `load_remote_resource` 的替身载荷：1×1 透明 PNG。
 *
 * 公告头图、贡献者头像等都经这个命令取字节，无 IPC 时会 reject 并让组件
 * 落到「图片加载失败」的占位。给一张极小 PNG，占位既能加载成功也不会
 * 撑出错误文案。
 */
const TINY_PNG_BASE64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==";

export const mockRemoteResourceBytes: number[] = Array.from(
  Uint8Array.from(atob(TINY_PNG_BASE64), (char) => char.charCodeAt(0)),
);
