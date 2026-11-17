import { useMemo } from "react";
import { Button, Card, LoadingIndicator } from "material/react";
import { LuCalendarClock, LuChevronRight, LuRotateCw } from "react-icons/lu";
import { Banner, Disclosure } from "../ui";
import { SkeletonText } from "../components";
import { useLogicalNavigation } from "../logical-navigation";
import { slotForSession } from "./periods";
import {
  formatClock,
  isPendingCheckin,
  localDateKey,
  nextSessionKey,
  sessionLocation,
  sessionStatus,
  signinRoute,
  teacherFor,
  todayKey,
  weekdayLabel,
} from "./merge";
import { SessionCard } from "./SessionCard";
import type { TimetableSession, TimetableSnapshot } from "./types";

/** 首页课表卡片的展示数据，由 `HomePage` 统一订阅后传入。 */
export interface HomeTimetableWidgetProps {
  snapshot?: TimetableSnapshot;
  error?: string;
  refreshing: boolean;
  refresh: () => void;
  /** 当前时间（毫秒），由页面级 `useNow` 统一推进，驱动课程状态翻转。 */
  now: number;
}

/**
 * 首页课表卡片：今日课程、当前/下一节课与签到状态。
 *
 * 与「日历与待办」的先后顺序由 `HomeCards` 按今日课程是否上完决定。
 * 详细课表在 `/study/timetable`。点击课程时，检测到签到任务就进签到页
 * 对应 tab；否则进课表页对应日期。
 */
export function HomeTimetableWidget({
  snapshot,
  error,
  refreshing,
  refresh,
  now,
}: HomeTimetableWidgetProps) {
  const navigation = useLogicalNavigation();

  const today = todayKey();
  const todaySessions = useMemo(
    () =>
      (snapshot?.sessions ?? [])
        .filter((session) => session.date === today)
        .sort((a, b) => a.startMs - b.startMs),
    [snapshot, today],
  );
  const next = useMemo(
    () =>
      (snapshot?.sessions ?? [])
        .filter((session) => session.startMs > now)
        .sort((a, b) => a.startMs - b.startMs)[0],
    [snapshot, now],
  );
  // 已上完的课默认折叠：首页只保留当前/未开始的课，历史课收进底部可展开区。
  const activeSessions = useMemo(
    () => todaySessions.filter((session) => session.endMs > now),
    [todaySessions, now],
  );
  const finishedSessions = useMemo(
    () => todaySessions.filter((session) => session.endMs <= now),
    [todaySessions, now],
  );
  const hasCurrent = todaySessions.some((session) => session.startMs <= now && now < session.endMs);
  const nextKey = hasCurrent ? undefined : nextSessionKey(snapshot?.sessions ?? [], now);
  const remaining = todaySessions.filter((session) => session.startMs > now).length;
  const pendingLmsToday = todaySessions.filter(
    (session) => session.checkin === "pending_lms",
  ).length;
  // SZU 待签条目对应课表里的同一课次，已在卡片上显示，不能重复计入「另有」。
  const lmsCheckins = (snapshot?.checkins ?? []).filter((item) =>
    item.id.startsWith("checkin:lms:"),
  ).length;
  const unmatchedCheckins = Math.max(0, lmsCheckins - pendingLmsToday);
  const dateLabel = `${new Date().getMonth() + 1}月${new Date().getDate()}日 ${weekdayLabel(
    new Date(),
  )}`;
  // 首次推送到达前既不是「没有课」也不是「已经上完」，用骨架屏占位。
  const initialLoading = snapshot === undefined && error === undefined;

  const openSession = (session: TimetableSession) => {
    const target = signinRoute(session, session.checkin, isPendingCheckin(session.checkin));
    if (target) {
      void navigation.push(target);
      return;
    }
    void navigation.push(
      `/study/timetable?tab=week&date=${localDateKey(new Date(session.startMs))}`,
    );
  };

  const cardFor = (session: TimetableSession) => {
    const slot = slotForSession(session.startMs, session.endMs);
    return (
      <SessionCard
        key={`${session.courseName}-${session.startMs}`}
        title={session.courseName}
        periodLabel={slot?.periodLabel ?? "其他时段"}
        timeLabel={`${formatClock(session.startMs)}–${formatClock(session.endMs)}`}
        teacher={snapshot ? teacherFor(session, snapshot.semesterCourses) : undefined}
        location={sessionLocation(session)}
        checkin={session.checkin}
        status={sessionStatus(session, now, nextKey)}
        onClick={() => openSession(session)}
      />
    );
  };

  return (
    <Card
      className="home-timetable-widget p-4 sm:p-6"
      variant="outlined"
      aria-labelledby="home-timetable-heading"
    >
      <div className="flex items-center justify-between gap-4 mb-4">
        <h2 id="home-timetable-heading" className="text-lg font-medium flex items-center gap-2">
          <LuCalendarClock className="text-primary" aria-hidden="true" />
          <span>课表</span>
        </h2>
        <div className="flex items-center gap-2">
          {refreshing && (
            <span className="text-sm text-muted flex items-center gap-1">
              <LoadingIndicator size={14} /> 刷新中
            </span>
          )}
          <Button
            variant="text"
            className="text-xs"
            onClick={() => void refresh()}
            aria-label="刷新课表"
          >
            <LuRotateCw aria-hidden="true" />
          </Button>
          <Button
            variant="text"
            className="text-xs"
            onClick={() => void navigation.push("/study/timetable")}
          >
            查看全部
            <LuChevronRight aria-hidden="true" />
          </Button>
        </div>
      </div>

      <div className="grid gap-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-sm text-muted">
            {dateLabel}
            {snapshot?.semester ? ` · ${snapshot.semester}` : ""}
          </p>
          {snapshot && <p className="text-xs text-muted">今天还剩 {remaining} 节课要上</p>}
        </div>

        {(error || snapshot?.message) && (
          <Banner status="warning">
            <span className="text-sm">{error ?? snapshot?.message}</span>
          </Banner>
        )}

        {unmatchedCheckins > 0 && (
          <Button
            variant="text"
            className="justify-start text-xs"
            onClick={() => void navigation.push("/study/signin?tab=lms")}
          >
            另有 {unmatchedCheckins} 个待签点名
          </Button>
        )}

        {initialLoading ? (
          <div
            className="grid gap-2 rounded-inner border border-separator p-4"
            role="status"
            aria-label="正在读取课表"
          >
            <SkeletonText lines={3} />
          </div>
        ) : snapshot === undefined ? (
          <div className="grid gap-2 rounded-inner border border-separator p-4">
            <p className="text-sm">课表暂不可用，请稍后重试。</p>
          </div>
        ) : todaySessions.length > 0 ? (
          <div className="grid gap-2">
            {/* 按时间顺序：已上完（默认折叠）→ 正在上/未上 → 明天的下一节。 */}
            {finishedSessions.length > 0 && (
              <Disclosure
                title={`已上过的课 · ${finishedSessions.length} 节`}
                ariaLabel="展开已上过的课"
              >
                <div className="grid gap-2">{finishedSessions.map(cardFor)}</div>
              </Disclosure>
            )}
            {activeSessions.map(cardFor)}
            {next && next.date !== today && (
              <div className="grid gap-1 pt-1">
                <p className="text-xs text-muted">下一节 · {next.date}</p>
                {cardFor(next)}
              </div>
            )}
          </div>
        ) : (
          <div className="grid gap-2 rounded-inner border border-separator p-4">
            <p className="text-sm">今天没有课。</p>
            {next && (
              <>
                <p className="text-xs text-muted">下一节 · {next.date}</p>
                {cardFor(next)}
              </>
            )}
          </div>
        )}
      </div>
    </Card>
  );
}
