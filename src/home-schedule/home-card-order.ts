/** 首页两张可换位卡片的标识。 */
export type HomeCardOrder = "timetable" | "calendar";

export interface HomeCardOrderInput {
  /** 仅今天的课次；空数组表示今天没有课。 */
  todaySessions: readonly { endMs: number }[];
  /** 当前时间（毫秒），由 `useNow` 推进。 */
  now: number;
  /** 后端聚合出的待办是否非空。 */
  hasTodos: boolean;
  /** 两份快照都已完成首次推送；未就绪时保持默认顺序，避免加载时闪跳。 */
  ready: boolean;
}

/**
 * 决定首页课表与待办卡片谁在上。
 *
 * 课程是时间敏感的：只要今天还有课没上完，课表保持在最上面；
 * 最后一节课结束后（或今天本来就没课），把待办提到上面。
 * 没有待办时保持默认顺序，避免把一张空卡片放到首屏。
 */
export function resolveHomeCardOrder({
  todaySessions,
  now,
  hasTodos,
  ready,
}: HomeCardOrderInput): HomeCardOrder {
  if (!ready || !hasTodos) return "timetable";
  if (todaySessions.length === 0) return "calendar";
  return todaySessions.every((session) => session.endMs <= now) ? "calendar" : "timetable";
}
