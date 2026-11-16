export type PeriodRow = {
  id: string;
  period: "上午" | "下午" | "晚上";
  session: string;
  time: string;
  /** 当天分钟数，用于与考勤课次匹配。大课间没有节次。 */
  startMinute: number;
  endMinute: number;
  periods: number[];
};

// 深圳大学教务部标准作息，2026-09-29 核对。晚上两个合并块保持原样。
export const PERIOD_ROWS: readonly PeriodRow[] = [
  {
    id: "lesson-1",
    period: "上午",
    session: "第一节",
    time: "8:30 — 9:10",
    startMinute: 510,
    endMinute: 550,
    periods: [1],
  },
  {
    id: "lesson-2",
    period: "上午",
    session: "第二节",
    time: "9:15 — 9:55",
    startMinute: 555,
    endMinute: 595,
    periods: [2],
  },
  {
    id: "morning-break",
    period: "上午",
    session: "大课间",
    time: "20 分钟",
    startMinute: 595,
    endMinute: 615,
    periods: [],
  },
  {
    id: "lesson-3",
    period: "上午",
    session: "第三节",
    time: "10:15 — 10:55",
    startMinute: 615,
    endMinute: 655,
    periods: [3],
  },
  {
    id: "lesson-4",
    period: "上午",
    session: "第四节",
    time: "11:00 — 11:40",
    startMinute: 660,
    endMinute: 700,
    periods: [4],
  },
  {
    id: "lesson-5",
    period: "上午",
    session: "第五节",
    time: "11:45 — 12:25",
    startMinute: 705,
    endMinute: 745,
    periods: [5],
  },
  {
    id: "lesson-6",
    period: "下午",
    session: "第六节",
    time: "13:30 — 14:10",
    startMinute: 810,
    endMinute: 850,
    periods: [6],
  },
  {
    id: "lesson-7",
    period: "下午",
    session: "第七节",
    time: "14:15 — 14:55",
    startMinute: 855,
    endMinute: 895,
    periods: [7],
  },
  {
    id: "lesson-8",
    period: "下午",
    session: "第八节",
    time: "15:00 — 15:40",
    startMinute: 900,
    endMinute: 940,
    periods: [8],
  },
  {
    id: "afternoon-break",
    period: "下午",
    session: "大课间",
    time: "20 分钟",
    startMinute: 940,
    endMinute: 960,
    periods: [],
  },
  {
    id: "lesson-9",
    period: "下午",
    session: "第九节",
    time: "16:00 — 16:40",
    startMinute: 960,
    endMinute: 1000,
    periods: [9],
  },
  {
    id: "lesson-10",
    period: "下午",
    session: "第十节",
    time: "16:45 — 17:25",
    startMinute: 1005,
    endMinute: 1045,
    periods: [10],
  },
  {
    id: "lessons-11-12",
    period: "晚上",
    session: "第十一、十二节",
    time: "19:00 — 20:20",
    startMinute: 1140,
    endMinute: 1220,
    periods: [11, 12],
  },
  {
    id: "lessons-13-14",
    period: "晚上",
    session: "第十三、十四节",
    time: "20:30 — 21:45",
    startMinute: 1230,
    endMinute: 1305,
    periods: [13, 14],
  },
];

/** 有具体节次的作息块（本周网格的行）。 */
export const PERIOD_BLOCKS: readonly PeriodRow[] = PERIOD_ROWS.filter(
  (row) => row.periods.length > 0,
);

export function periodLabel(row: PeriodRow): string {
  if (row.periods.length === 0) return row.session;
  const first = row.periods[0];
  const last = row.periods[row.periods.length - 1];
  return first === last ? `第${first}节` : `第${first}–${last}节`;
}

export function rowTimeLabel(row: PeriodRow): string {
  return `${formatMinute(row.startMinute)}–${formatMinute(row.endMinute)}`;
}

export function formatMinute(total: number): string {
  const hours = Math.floor(total / 60);
  const minutes = total % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

export function minuteOfDay(ms: number): number {
  const date = new Date(ms);
  return date.getHours() * 60 + date.getMinutes();
}

/**
 * 把考勤课次映射到作息块。
 *
 * 上课时间按「开始时刻」匹配到最近的作息块（±15 分钟），结束时刻用来推算
 * 跨越了几个节次（±20 分钟）。一门 08:30–10:05 的课会落在第 1 节行，
 * 标签显示「第1–2节」；完全匹配不到时返回 undefined，由页面放进「其他时段」。
 */
export type SessionSlot = {
  row: PeriodRow;
  periods: number[];
  periodLabel: string;
  timeLabel: string;
};

export function slotForSession(startMs: number, endMs: number): SessionSlot | undefined {
  const start = minuteOfDay(startMs);
  const end = minuteOfDay(endMs);
  let row: PeriodRow | undefined;
  let best = Number.POSITIVE_INFINITY;
  for (const block of PERIOD_BLOCKS) {
    const distance = Math.abs(block.startMinute - start);
    if (distance <= 15 && distance < best) {
      row = block;
      best = distance;
    }
  }
  if (!row) return undefined;

  let endRow: PeriodRow | undefined;
  best = Number.POSITIVE_INFINITY;
  for (const block of PERIOD_BLOCKS) {
    const distance = Math.abs(block.endMinute - end);
    if (distance <= 20 && distance < best) {
      endRow = block;
      best = distance;
    }
  }

  const first = row.periods[0];
  const last = Math.max(
    first,
    endRow ? endRow.periods[endRow.periods.length - 1] : row.periods[row.periods.length - 1],
  );
  const periods = Array.from({ length: last - first + 1 }, (_, index) => first + index);
  return {
    row,
    periods,
    periodLabel: first === last ? `第${first}节` : `第${first}–${last}节`,
    timeLabel: `${formatMinute(start)}–${formatMinute(end)}`,
  };
}

export function periodRowForPeriod(period: number): PeriodRow | undefined {
  return PERIOD_BLOCKS.find((row) => row.periods.includes(period));
}

/** 节次在作息网格里的行下标（0 = 第一节所在行）。 */
export function rowIndexForPeriod(period: number): number | undefined {
  const index = PERIOD_BLOCKS.findIndex((row) => row.periods.includes(period));
  return index < 0 ? undefined : index;
}
