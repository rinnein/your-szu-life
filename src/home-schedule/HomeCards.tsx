import { useEffect, useMemo, useState } from "react";
import { motion, useReducedMotion } from "motion/react";
import { spatial } from "../motion/tokens";
import {
  HomeTimetableWidget,
  type HomeTimetableWidgetProps,
} from "../timetable/HomeTimetableWidget";
import { HomeCalendarWidget, type HomeCalendarWidgetProps } from "./HomeCalendarWidget";
import { resolveHomeCardOrder, type HomeCardOrder } from "./home-card-order";
import { localDateKey } from "../timetable/merge";

export interface HomeCardsProps {
  /** 当前时间（毫秒），由 `HomePage` 的 `useNow` 推进。 */
  now: number;
  timetable: Omit<HomeTimetableWidgetProps, "now">;
  /** 待办数据；`ready` 表示首次 Channel 推送是否已经到达。 */
  calendar: HomeCalendarWidgetProps;
}

/**
 * 首页的课表 / 日历与待办两张卡片。
 *
 * 今天还有课没上完时课表在上；最后一节课结束后（或今天没课）且有
 * 待办时换成待办在上，没有待办则保持默认顺序。换位用 M3 弹簧做
 * 布局动画，`layoutDependency` 保证只有顺序真正变化时才重新测位；
 * 数据首次到达那一次不播动画，避免卡片高度还没稳定时换位。
 */
export function HomeCards({ now, timetable, calendar }: HomeCardsProps) {
  const reduceMotion = useReducedMotion();
  const today = localDateKey(new Date(now));
  const { items, categoryMap, loading, refresh, ready, initialDate } = calendar;
  const dataReady = ready && (timetable.snapshot !== undefined || timetable.error !== undefined);
  // 首次拿到数据时顺序可能从默认值翻转，这一次不播动画；之后的下课换位才播。
  const [layoutReady, setLayoutReady] = useState(false);

  useEffect(() => {
    if (dataReady) setLayoutReady(true);
  }, [dataReady]);

  const order = useMemo<HomeCardOrder>(
    () =>
      resolveHomeCardOrder({
        todaySessions: (timetable.snapshot?.sessions ?? []).filter(
          (session) => session.date === today,
        ),
        now,
        hasTodos: items.length > 0,
        ready: dataReady,
      }),
    [timetable.snapshot, items.length, dataReady, now, today],
  );

  const cards: HomeCardOrder[] =
    order === "calendar" ? ["calendar", "timetable"] : ["timetable", "calendar"];

  return (
    <>
      {cards.map((card) => (
        <motion.div
          key={card}
          layout={reduceMotion || !layoutReady ? false : "position"}
          layoutDependency={order}
          transition={spatial.default}
        >
          {card === "timetable" ? (
            <HomeTimetableWidget {...timetable} now={now} />
          ) : (
            <HomeCalendarWidget
              items={items}
              categoryMap={categoryMap}
              loading={loading}
              ready={ready}
              refresh={refresh}
              initialDate={initialDate}
            />
          )}
        </motion.div>
      ))}
    </>
  );
}
