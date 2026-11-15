import type { UoocSnapshot } from "../state/types";

/**
 * 讨论完成率的唯一计算点（此前在页面里重复了三遍且口径不一）。
 *
 * 返回 undefined 表示没有可判定的权重数据；<1 才需要继续参与讨论。
 */
export function discussionRatio(snapshot: UoocSnapshot | undefined, cid: string): number | undefined {
  const progress = snapshot?.progress?.[cid]?.data?.discussion;
  if (progress?.score == null || progress.weight == null || progress.weight <= 0) return undefined;
  return progress.score / progress.weight;
}

/** 课程是否还需要参与讨论（复习模式课程永不参与）。 */
export function discussionNeeded(snapshot: UoocSnapshot | undefined, cid: string): boolean {
  const course = snapshot?.courses.find((c) => c.id === cid);
  if (course?.status === "33") return false;
  const ratio = discussionRatio(snapshot, cid);
  return ratio != null && ratio < 1;
}

/** 有待完成讨论的课程列表，按课程顺序。 */
export function discussionCandidates(snapshot: UoocSnapshot | undefined) {
  return (snapshot?.courses ?? []).filter((course) => discussionNeeded(snapshot, course.id));
}

/** 讨论状态的一句话说明；无需讨论时返回 undefined（不再展示「无需讨论」行）。 */
export function discussionStatusText(
  snapshot: UoocSnapshot | undefined,
  cid: string,
  now: number,
): string | undefined {
  const state = snapshot?.discussions?.[cid];
  if (!discussionNeeded(snapshot, cid)) return undefined;
  if (state?.lastSentAtMs) {
    const elapsed = Math.max(0, now - state.lastSentAtMs);
    return `上次发送讨论于${elapsed < 60_000 ? "不到 1 分钟前" : `${Math.floor(elapsed / 60_000)} 分钟前`}`;
  }
  return "满足条件后会自动发起";
}
