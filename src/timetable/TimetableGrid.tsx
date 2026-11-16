import type { ReactNode } from "react";
import { PERIOD_BLOCKS, formatMinute, periodLabel } from "./periods";

export type GridDay = {
  key: string;
  label: string;
  sublabel?: string;
  /** 今天：日期与整列高亮。 */
  today?: boolean;
};

export type GridItem = {
  key: string;
  /** 星期下标，0 = 周一。 */
  dayIndex: number;
  /** 作息块下标，0 = 第一节。 */
  startRow: number;
  /** 占用的作息块数量。 */
  span: number;
  content: ReactNode;
};

/**
 * 作息 × 星期的课表网格。
 *
 * 每一列是一天，每一行是作息表里的一节课；没有课的格子保持为空。
 * 列宽按容器均分，窄屏下 7 天仍然全部可见，不需要横向滚动。
 */
export function TimetableGrid({ days, items }: { days: GridDay[]; items: GridItem[] }) {
  const covered = new Set<string>();
  for (const item of items) {
    for (let row = item.startRow; row < item.startRow + item.span; row += 1) {
      covered.add(`${item.dayIndex}:${row}`);
    }
  }

  return (
    <div className="overflow-hidden rounded-inner border border-separator bg-separator">
      <div
        className="grid auto-rows-[3.25rem] gap-px lg:auto-rows-[4.25rem]"
        style={{ gridTemplateColumns: `2.75rem repeat(${days.length}, minmax(0, 1fr))` }}
      >
        <div
          className="flex flex-col items-center justify-center bg-surface text-[10px] text-muted"
          style={{ gridColumn: 1, gridRow: 1 }}
        >
          节次
        </div>
        {days.map((day, index) => (
          <div
            key={day.key}
            className={`flex min-w-0 flex-col items-center justify-center gap-0.5 px-1 py-1 text-center ${
              day.today ? "bg-accent-soft text-primary" : "bg-surface"
            }`}
            style={{ gridColumn: index + 2, gridRow: 1 }}
          >
            <span className="text-[11px] font-medium lg:text-xs">{day.label}</span>
            {day.sublabel && (
              <span
                className={`text-[10px] tabular-nums ${day.today ? "" : "text-muted"}`}
                aria-hidden="true"
              >
                {day.sublabel}
              </span>
            )}
          </div>
        ))}

        {PERIOD_BLOCKS.map((row, index) => (
          <div
            key={row.id}
            className="flex flex-col items-center justify-center gap-0.5 bg-surface px-1 text-center"
            style={{ gridColumn: 1, gridRow: index + 2 }}
          >
            <span className="text-[10px] font-medium tabular-nums lg:text-[11px]">
              {periodLabel(row)}
            </span>
            <span className="text-[9px] text-muted tabular-nums" aria-hidden="true">
              {formatMinute(row.startMinute)}
            </span>
          </div>
        ))}

        {days.map((day, dayIndex) =>
          PERIOD_BLOCKS.map((row, rowIndex) =>
            covered.has(`${dayIndex}:${rowIndex}`) ? null : (
              <div
                key={`${day.key}:${row.id}`}
                className="bg-surface"
                style={{ gridColumn: dayIndex + 2, gridRow: rowIndex + 2 }}
              />
            ),
          ),
        )}

        {items.map((item) => (
          <div
            key={item.key}
            className="min-w-0 bg-surface p-0.5"
            style={{
              gridColumn: item.dayIndex + 2,
              gridRow: `${item.startRow + 2} / span ${Math.max(1, item.span)}`,
            }}
          >
            <div className="h-full min-h-0">{item.content}</div>
          </div>
        ))}
      </div>
    </div>
  );
}
