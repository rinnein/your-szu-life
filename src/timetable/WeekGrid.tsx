import { useEffect, useMemo, useState } from "react";
import { Button } from "material/react";
import { LuChevronLeft, LuChevronRight } from "react-icons/lu";
import { useLogicalNavigation } from "../logical-navigation";
import type { SemesterCourse, TimetableSession } from "./types";
import { rowIndexForPeriod, slotForSession } from "./periods";
import {
  addDays,
  datesOfWeek,
  formatClock,
  isPendingCheckin,
  localDateKey,
  nextSessionKey,
  placeInGrid,
  sessionLocation,
  sessionStatus,
  shortDateLabel,
  signinRoute,
  startOfWeek,
  teacherFor,
  todayKey,
  weekdayLabel,
} from "./merge";
import { useNow } from "./use-now";
import { SessionCard } from "./SessionCard";
import { SessionCell } from "./SessionCell";
import { SessionDetailDialog } from "./SessionDetailDialog";
import { TimetableGrid, type GridDay, type GridItem } from "./TimetableGrid";

type Props = {
  sessions: TimetableSession[];
  semesterCourses: SemesterCourse[];
  initialDate?: Date;
};

/**
 * 本周视图：作息 × 星期的网格。
 *
 * 列是一周七天，行是作息表里的每一节课，没有课的格子留空；列宽随容器均分，
 * 窄屏也能一屏看完一周，不需要横向滚动。完整的教师/时间/签到信息在点击
 * 课程格后弹出的详情里。
 */
export function WeekGrid({ sessions, semesterCourses, initialDate }: Props) {
  const navigation = useLogicalNavigation();
  const now = useNow();
  const [weekStart, setWeekStart] = useState(() => startOfWeek(initialDate ?? new Date()));
  const [active, setActive] = useState<TimetableSession | null>(null);

  useEffect(() => {
    if (initialDate) setWeekStart(startOfWeek(initialDate));
  }, [initialDate]);

  const dates = useMemo(() => datesOfWeek(weekStart), [weekStart]);
  const dayKeys = useMemo(() => dates.map(localDateKey), [dates]);
  const today = todayKey();

  const days: GridDay[] = useMemo(
    () =>
      dates.map((date, index) => ({
        key: dayKeys[index],
        label: weekdayLabel(date),
        sublabel: shortDateLabel(date),
        today: dayKeys[index] === today,
      })),
    [dates, dayKeys, today],
  );

  const bounds = useMemo(() => {
    if (sessions.length === 0) return undefined;
    const starts = sessions.map((session) => session.startMs);
    const ends = sessions.map((session) => session.endMs);
    return {
      min: startOfWeek(new Date(Math.min(...starts))),
      max: startOfWeek(new Date(Math.max(...ends))),
    };
  }, [sessions]);

  const thisWeek = startOfWeek(new Date());
  const canPrev = !bounds || weekStart > bounds.min;
  const canNext = !bounds || weekStart < bounds.max;
  const hasCurrent = sessions.some((session) => session.startMs <= now && now < session.endMs);
  const nextKey = hasCurrent ? undefined : nextSessionKey(sessions, now);

  const { placements, extras } = useMemo(() => {
    const entries: {
      session: TimetableSession;
      dayIndex: number;
      startRow: number;
      endRow: number;
    }[] = [];
    const extras: TimetableSession[] = [];
    for (const session of sessions) {
      const dayIndex = dayKeys.indexOf(session.date);
      if (dayIndex < 0) continue;
      const slot = slotForSession(session.startMs, session.endMs);
      const startRow = slot ? rowIndexForPeriod(slot.periods[0]) : undefined;
      const endRow = slot ? rowIndexForPeriod(slot.periods[slot.periods.length - 1]) : undefined;
      if (startRow === undefined || endRow === undefined) {
        extras.push(session);
        continue;
      }
      entries.push({ session, dayIndex, startRow, endRow });
    }
    return { placements: placeInGrid(entries), extras };
  }, [sessions, dayKeys]);

  const items: GridItem[] = useMemo(
    () =>
      placements.map((placement) => ({
        key: `${placement.dayIndex}:${placement.startRow}`,
        dayIndex: placement.dayIndex,
        startRow: placement.startRow,
        span: placement.span,
        content: (
          <div className="flex h-full min-h-0 flex-col gap-0.5">
            {[...placement.entries]
              .sort((a, b) => a.session.startMs - b.session.startMs)
              .map(({ session, startRow, endRow }) => (
                <SessionCell
                  key={`${session.courseName}-${session.startMs}`}
                  title={session.courseName}
                  location={sessionLocation(session)}
                  teacher={teacherFor(session, semesterCourses)}
                  timeLabel={`${formatClock(session.startMs)}–${formatClock(session.endMs)}`}
                  checkin={session.checkin}
                  status={sessionStatus(session, now, nextKey)}
                  span={endRow - startRow + 1}
                  onClick={() => setActive(session)}
                />
              ))}
          </div>
        ),
      })),
    [placements, now, nextKey, semesterCourses],
  );

  return (
    <div className="grid gap-3" data-testid="week-grid">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-1">
          <Button
            variant="text"
            disabled={!canPrev}
            onClick={() => setWeekStart(addDays(weekStart, -7))}
            aria-label="上一周"
          >
            <LuChevronLeft aria-hidden="true" />
          </Button>
          <Button
            variant="text"
            disabled={weekStart.getTime() === thisWeek.getTime()}
            onClick={() => setWeekStart(thisWeek)}
          >
            本周
          </Button>
          <Button
            variant="text"
            disabled={!canNext}
            onClick={() => setWeekStart(addDays(weekStart, 7))}
            aria-label="下一周"
          >
            <LuChevronRight aria-hidden="true" />
          </Button>
          <span className="ml-1 text-sm text-muted tabular-nums">
            {shortDateLabel(dates[0])} – {shortDateLabel(dates[6])}
          </span>
        </div>
      </div>

      <TimetableGrid days={days} items={items} />

      {sessions.length === 0 && (
        <p className="text-sm text-muted">
          考勤课表中还没有课程。请先在「签到」页刷新 SZU 课表，再回到这里查看。
        </p>
      )}

      {extras.length > 0 && (
        <section className="grid gap-2" aria-label="其他时段">
          <h3 className="text-sm font-medium text-muted">其他时段</h3>
          <div className="grid gap-2 sm:grid-cols-2">
            {extras.map((session) => (
              <SessionCard
                key={`${session.courseName}-${session.startMs}`}
                title={session.courseName}
                periodLabel="其他时段"
                timeLabel={`${formatClock(session.startMs)}–${formatClock(session.endMs)}`}
                teacher={teacherFor(session, semesterCourses)}
                location={sessionLocation(session)}
                checkin={session.checkin}
                status={sessionStatus(session, now, nextKey)}
                note={`${weekdayLabel(new Date(session.startMs))} ${session.date}`}
                onClick={() => setActive(session)}
              />
            ))}
          </div>
        </section>
      )}

      <SessionDetailDialog
        open={active !== null}
        title={active?.courseName ?? ""}
        periodLabel={active ? slotForSession(active.startMs, active.endMs)?.periodLabel : undefined}
        timeLabel={
          active ? `${formatClock(active.startMs)}–${formatClock(active.endMs)}` : undefined
        }
        teacher={active ? teacherFor(active, semesterCourses) : undefined}
        location={active ? sessionLocation(active) : undefined}
        checkin={active?.checkin ?? "none"}
        onClose={() => setActive(null)}
        onSignin={
          active
            ? () => {
                const target = signinRoute(
                  active,
                  active.checkin,
                  isPendingCheckin(active.checkin),
                );
                if (target) void navigation.push(target);
              }
            : undefined
        }
      />
    </div>
  );
}
