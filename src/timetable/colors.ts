import { normalizeCourseName } from "./merge";

/**
 * 课程分类色板：与首页待办同一套 seed 生成的 accent-1…12。
 *
 * 用课程名做稳定哈希，因此同一门课在「本周 / 学期 / 以后新增的视图」里
 * 永远得到同一种颜色，不同课程尽量落在不同色相上。
 */
export type CourseTone = {
  /** 卡片底色（accent 18% 混 surface）。 */
  soft: string;
  /** 左侧色条 / ghost 虚线边框颜色。 */
  bar: string;
};

const TONES: readonly CourseTone[] = [
  { soft: "bg-accent-1-soft", bar: "border-accent-1" },
  { soft: "bg-accent-2-soft", bar: "border-accent-2" },
  { soft: "bg-accent-3-soft", bar: "border-accent-3" },
  { soft: "bg-accent-4-soft", bar: "border-accent-4" },
  { soft: "bg-accent-5-soft", bar: "border-accent-5" },
  { soft: "bg-accent-6-soft", bar: "border-accent-6" },
  { soft: "bg-accent-7-soft", bar: "border-accent-7" },
  { soft: "bg-accent-8-soft", bar: "border-accent-8" },
  { soft: "bg-accent-9-soft", bar: "border-accent-9" },
  { soft: "bg-accent-10-soft", bar: "border-accent-10" },
  { soft: "bg-accent-11-soft", bar: "border-accent-11" },
  { soft: "bg-accent-12-soft", bar: "border-accent-12" },
];

export function courseTone(name: string): CourseTone {
  const key = normalizeCourseName(name) || name || "";
  let hash = 0;
  for (const char of key) {
    hash = (hash * 31 + (char.codePointAt(0) ?? 0)) % 1_000_003;
  }
  return TONES[hash % TONES.length];
}
