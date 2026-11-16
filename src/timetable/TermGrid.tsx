import { useMemo, useState } from "react";
import { Select, SelectOption } from "material/react";
import { Banner } from "../ui";
import type { SemesterCourse } from "./types";
import { formatMinute, periodRowForPeriod, rowIndexForPeriod } from "./periods";
import { formatWeeks, placeInGrid, semesterPeriodLabel } from "./merge";
import { courseTone } from "./colors";
import { SessionDetailDialog } from "./SessionDetailDialog";
import { TimetableGrid, type GridDay, type GridItem } from "./TimetableGrid";

type Props = {
  courses: SemesterCourse[];
  message: string;
  fetchedAtMs: number;
};

const DAYS: GridDay[] = [
  { key: "1", label: "周一" },
  { key: "2", label: "周二" },
  { key: "3", label: "周三" },
  { key: "4", label: "周四" },
  { key: "5", label: "周五" },
  { key: "6", label: "周六" },
  { key: "7", label: "周日" },
];

function semesterTimeLabel(course: SemesterCourse): string {
  const start = periodRowForPeriod(course.startPeriod);
  const end = periodRowForPeriod(course.endPeriod);
  if (!start || !end) return "";
  return `${formatMinute(start.startMinute)}–${formatMinute(end.endMinute)}`;
}

/**
 * 学期视图：eHall 固定课表的作息 × 星期网格。
 *
 * 数据由后端持久缓存、每天最多刷新一次；失败时继续展示上次缓存。
 * 刷新与重新认证都在页面顶栏，网格下方不再放操作按钮。
 */
export function TermGrid({ courses, message, fetchedAtMs }: Props) {
  const [week, setWeek] = useState("all");
  const [active, setActive] = useState<SemesterCourse | null>(null);

  const { items, unmatched } = useMemo(() => {
    const entries: {
      course: SemesterCourse;
      dayIndex: number;
      startRow: number;
      endRow: number;
    }[] = [];
    const unmatched: SemesterCourse[] = [];
    for (const course of courses) {
      if (week !== "all" && !course.weeks.includes(Number(week))) continue;
      const dayIndex = course.weekday - 1;
      const startRow = rowIndexForPeriod(course.startPeriod);
      const endRow = rowIndexForPeriod(course.endPeriod);
      if (dayIndex < 0 || dayIndex > 6 || startRow === undefined || endRow === undefined) {
        unmatched.push(course);
        continue;
      }
      entries.push({ course, dayIndex, startRow, endRow });
    }

    const items: GridItem[] = placeInGrid(entries).map((placement) => ({
      key: `${placement.dayIndex}:${placement.startRow}`,
      dayIndex: placement.dayIndex,
      startRow: placement.startRow,
      span: placement.span,
      content: (
        <div className="flex h-full min-h-0 flex-col gap-0.5">
          {placement.entries.map(({ course, startRow, endRow }) => {
            const tone = courseTone(course.name);
            const span = endRow - startRow + 1;
            const detail = [course.teacher, course.classroom].filter(Boolean).join(" · ");
            return (
              <button
                key={`${course.name}-${course.startPeriod}-${course.weekday}-${course.classroom}`}
                type="button"
                className={`flex min-h-0 w-full flex-1 flex-col gap-0.5 overflow-hidden rounded-md border-y-0 border-r-0 border-l-[3px] px-1 py-0.5 text-left leading-tight transition-colors hover:ring-1 hover:ring-primary/40 ${tone.bar} ${tone.soft}`}
                onClick={() => setActive(course)}
              >
                <span
                  className={`${span >= 3 ? "line-clamp-4" : span >= 2 ? "line-clamp-3" : "line-clamp-2"} min-w-0 break-words text-[10px] font-medium lg:text-xs`}
                >
                  {course.name}
                </span>
                {detail && (
                  <span
                    className={`${span >= 2 ? "block" : "hidden lg:block"} ${
                      span >= 3 ? "line-clamp-3" : "line-clamp-2"
                    } break-words text-[10px] text-muted`}
                  >
                    {detail}
                  </span>
                )}
                <span className="mt-auto block break-words text-[9px] text-muted">
                  {formatWeeks(course.weeks)}
                </span>
              </button>
            );
          })}
        </div>
      ),
    }));
    return { items, unmatched };
  }, [courses, week]);

  if (courses.length === 0) {
    return (
      <Banner status="warning">
        <Banner.Title>尚未获取到本学期固定课表</Banner.Title>
        <Banner.Description>
          {message ||
            "应用会通过统一认证自动访问教务系统；如果统一认证已失效，请重新登录后再点右上角刷新。"}
        </Banner.Description>
      </Banner>
    );
  }

  return (
    <div className="grid gap-3">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <p className="min-w-0 text-sm text-muted">
          已缓存 {courses.length} 个排课槽 · 每天最多刷新一次
          {fetchedAtMs > 0 && (
            <span className="tabular-nums">
              {" "}
              · 上次更新 {new Date(fetchedAtMs).toLocaleString()}
            </span>
          )}
        </p>
        <Select
          label="显示周次"
          aria-label="显示周次"
          className="w-full shrink-0 sm:w-52"
          value={week}
          onChange={(event: any) =>
            setWeek(String(event.detail?.value ?? event.target?.value ?? "all"))
          }
        >
          <SelectOption value="all">全部周次</SelectOption>
          {Array.from({ length: 30 }, (_, index) => (
            <SelectOption key={index + 1} value={String(index + 1)}>
              第 {index + 1} 周
            </SelectOption>
          ))}
        </Select>
      </div>

      {message && (
        <Banner status="info">
          <span className="text-sm">{message}</span>
        </Banner>
      )}

      <TimetableGrid days={DAYS} items={items} />

      {unmatched.length > 0 && (
        <p className="text-xs text-muted">
          另有 {unmatched.length} 门课的节次不在作息表里，未在网格显示。
        </p>
      )}

      <SessionDetailDialog
        open={active !== null}
        title={active?.name ?? ""}
        periodLabel={active ? semesterPeriodLabel(active) : undefined}
        timeLabel={active ? semesterTimeLabel(active) : undefined}
        teacher={active?.teacher}
        location={active?.classroom}
        weeks={active ? formatWeeks(active.weeks) : undefined}
        checkin="none"
        onClose={() => setActive(null)}
      />
    </div>
  );
}
