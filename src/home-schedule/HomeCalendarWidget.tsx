import { useMemo, useState } from "react";
import { Button, Card, LoadingIndicator } from "material/react";
import { Chip } from "../ui";
import {
  LuCalendar,
  LuChevronLeft,
  LuChevronRight,
  LuClock,
  LuArrowUpRight,
  LuRotateCw,
} from "react-icons/lu";
import { generateCalendarGrid, formatDateKey } from "./calendar-math";
import { useHomeDeadlines } from "./use-home-deadlines";
import { useLogicalNavigation } from "../logical-navigation";
import type { ScheduleCategory } from "./types";

const WEEKDAYS = ["日", "一", "二", "三", "四", "五", "六"];

function formatDeadlineText(date: Date, now: Date = new Date()): string {
  const diffMs = date.getTime() - now.getTime();
  const month = date.getMonth() + 1;
  const day = date.getDate();
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  const timeStr = `${month}月${day}日 ${hours}:${minutes}`;

  if (diffMs < 0) return `${timeStr} (已截止)`;
  if (diffMs < 3600_000) return `即将截止 (剩余 ${Math.ceil(diffMs / 60_000)} 分钟)`;
  if (diffMs < 86400_000) return `今日截止 (${hours}:${minutes})`;
  if (diffMs < 86400_000 * 2) return `明日截止 (${hours}:${minutes})`;

  const daysLeft = Math.ceil(diffMs / 86400_000);
  return `${timeStr} (剩余 ${daysLeft} 天)`;
}

function getCategoryDotClass(category: ScheduleCategory): string {
  switch (category) {
    case "oj-quiz":
      return "bg-primary";
    case "electricity-outage":
      return "bg-danger";
    case "uooc-exam":
      return "bg-warning";
    case "uooc-homework":
      return "bg-success";
    case "uooc-course-end":
      return "bg-info";
  }
}

function renderCategoryChip(category: ScheduleCategory) {
  switch (category) {
    case "oj-quiz":
      return (
        <Chip size="sm" color="accent" variant="soft">
          OJ测验
        </Chip>
      );
    case "electricity-outage":
      return (
        <Chip size="sm" color="danger" variant="soft">
          电量
        </Chip>
      );
    case "uooc-exam":
      return (
        <Chip size="sm" color="warning" variant="soft">
          UOOC考试
        </Chip>
      );
    case "uooc-homework":
      return (
        <Chip size="sm" color="success" variant="soft">
          UOOC作业
        </Chip>
      );
    case "uooc-course-end":
      return (
        <Chip size="sm" color="info" variant="soft">
          UOOC结课
        </Chip>
      );
  }
}

function getCategoryAriaLabel(category: ScheduleCategory): string {
  switch (category) {
    case "oj-quiz":
      return "OJ测验";
    case "electricity-outage":
      return "电量预警";
    case "uooc-exam":
      return "UOOC考试";
    case "uooc-homework":
      return "UOOC作业";
    case "uooc-course-end":
      return "UOOC结课";
  }
}

export function HomeCalendarWidget({ initialDate }: { initialDate?: Date }) {
  const navigation = useLogicalNavigation();
  const { items, categoryMap, loading, refresh } = useHomeDeadlines();

  const [currentViewDate, setCurrentViewDate] = useState<Date>(() => initialDate ?? new Date());
  const [selectedDateKey, setSelectedDateKey] = useState<string | null>(null);

  const viewYear = currentViewDate.getFullYear();
  const viewMonth = currentViewDate.getMonth();

  const prevMonth = () => {
    setCurrentViewDate(new Date(viewYear, viewMonth - 1, 1));
  };

  const nextMonth = () => {
    setCurrentViewDate(new Date(viewYear, viewMonth + 1, 1));
  };

  const jumpToToday = () => {
    const today = new Date();
    setCurrentViewDate(today);
    setSelectedDateKey(formatDateKey(today));
  };

  const calendarGrid = useMemo(() => {
    return generateCalendarGrid(viewYear, viewMonth, selectedDateKey, categoryMap);
  }, [viewYear, viewMonth, selectedDateKey, categoryMap]);

  // 根据当前选中的日期过滤右侧待办
  const filteredItems = useMemo(() => {
    if (!selectedDateKey) return items;
    return items.filter((item) => item.dateKey === selectedDateKey);
  }, [items, selectedDateKey]);

  const formattedSelected = useMemo(() => {
    if (!selectedDateKey) return null;
    const parts = selectedDateKey.split("-");
    if (parts.length < 3) return selectedDateKey;
    return `${Number(parts[1])}月${Number(parts[2])}日`;
  }, [selectedDateKey]);

  return (
    <Card
      className="home-calendar-widget p-4 sm:p-6"
      variant="outlined"
      aria-labelledby="calendar-schedule-heading"
    >
      <div className="flex items-center justify-between gap-4 mb-4">
        <h2 id="calendar-schedule-heading" className="text-lg font-medium flex items-center gap-2">
          <LuCalendar className="text-primary" aria-hidden="true" />
          <span>日历与待办</span>
        </h2>
        <div className="flex items-center gap-2">
          {loading && (
            <span className="text-sm text-muted flex items-center gap-1">
              <LoadingIndicator size={14} /> 加载中
            </span>
          )}
          <Button variant="text" className="text-xs" onClick={jumpToToday} aria-label="回到今天">
            今天
          </Button>
          <Button
            variant="text"
            className="text-xs"
            onClick={() => void refresh()}
            aria-label="刷新日程"
          >
            <LuRotateCw aria-hidden="true" />
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-12 gap-6">
        {/* 左侧日历栏 (md: 6 列，lg: 5 列) */}
        <div className="md:col-span-6 lg:col-span-5 flex flex-col">
          <div className="flex items-center justify-between mb-3 px-1">
            <span className="font-semibold text-base">
              {viewYear}年 {viewMonth + 1}月
            </span>
            <div className="flex items-center gap-1">
              <Button
                variant="text"
                onClick={prevMonth}
                aria-label="上个月"
                className="!p-1 !min-w-8 !h-8"
              >
                <LuChevronLeft className="text-base" aria-hidden="true" />
              </Button>
              <Button
                variant="text"
                onClick={nextMonth}
                aria-label="下个月"
                className="!p-1 !min-w-8 !h-8"
              >
                <LuChevronRight className="text-base" aria-hidden="true" />
              </Button>
            </div>
          </div>

          {/* 星期行 */}
          <div className="grid grid-cols-7 text-center mb-1 text-xs text-muted font-medium">
            {WEEKDAYS.map((name) => (
              <div key={name} className="py-1">
                {name}
              </div>
            ))}
          </div>

          {/* 42 格日历网格 */}
          <div className="grid grid-cols-7 gap-y-1 gap-x-1" role="grid" aria-label="月历">
            {calendarGrid.map((cell) => {
              const fullDateLabel = `${cell.date.getFullYear()}年${cell.date.getMonth() + 1}月${cell.date.getDate()}日`;
              const isSelected = cell.isSelected;

              return (
                <button
                  key={cell.dateKey + (cell.isCurrentMonth ? "" : "-outside")}
                  type="button"
                  onClick={() => {
                    setSelectedDateKey((prev) => (prev === cell.dateKey ? null : cell.dateKey));
                  }}
                  className={`relative flex flex-col items-center justify-center h-10 w-full rounded-full transition-colors text-sm font-normal ${
                    !cell.isCurrentMonth
                      ? "text-muted opacity-40 hover:opacity-70"
                      : "text-foreground"
                  } ${
                    cell.isToday && !isSelected
                      ? "border border-primary font-bold text-primary"
                      : ""
                  } ${
                    isSelected
                      ? "bg-primary text-on-primary font-bold hover:bg-primary/90"
                      : "hover:bg-surface-container-high"
                  }`}
                  aria-label={fullDateLabel}
                  aria-selected={isSelected}
                  aria-current={cell.isToday ? "date" : undefined}
                >
                  <span>{cell.day}</span>

                  {/* 多类别圆点指示器 */}
                  <span className="absolute bottom-1 flex items-center justify-center gap-0.5">
                    {cell.categories.map((category) => (
                      <span
                        key={category}
                        className={`w-1.5 h-1.5 rounded-full ${
                          isSelected ? "bg-on-primary" : getCategoryDotClass(category)
                        }`}
                        aria-hidden="true"
                      />
                    ))}
                  </span>
                </button>
              );
            })}
          </div>

          {/* 图例说明 */}
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 mt-3 px-1 text-xs text-muted">
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-primary" aria-hidden="true" />
              <span>OJ 测验</span>
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-danger" aria-hidden="true" />
              <span>电量预警</span>
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-warning" aria-hidden="true" />
              <span>UOOC 考试</span>
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-success" aria-hidden="true" />
              <span>UOOC 作业</span>
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-info" aria-hidden="true" />
              <span>UOOC 结课</span>
            </span>
          </div>
        </div>

        {/* 右侧待办列表栏 (md: 6 列，lg: 7 列) */}
        <div className="md:col-span-6 lg:col-span-7 flex flex-col border-t md:border-t-0 md:border-l border-outline-variant pt-4 md:pt-0 md:pl-6">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <span className="font-semibold text-base">待办事项</span>
              <Chip size="sm" color="default" variant="soft">
                {filteredItems.length}
              </Chip>
            </div>
            {selectedDateKey && (
              <Button variant="text" className="text-xs" onClick={() => setSelectedDateKey(null)}>
                全部待办
              </Button>
            )}
          </div>

          {formattedSelected && (
            <div className="text-xs text-muted mb-2">
              正在查看：<span className="font-medium text-foreground">{formattedSelected}</span>{" "}
              的截止
            </div>
          )}

          <div className="flex flex-col gap-2.5 overflow-y-auto max-h-[360px] pr-1">
            {filteredItems.length === 0 ? (
              <div className="py-8 text-center text-sm text-muted">
                {selectedDateKey ? "该日无截止事项" : "近期暂无待办事项，一切就绪"}
              </div>
            ) : (
              filteredItems.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => void navigation.push(item.actionUrl)}
                  className="w-full text-left p-3 rounded-lg border border-outline-variant/60 hover:border-primary hover:bg-surface-container-low transition-colors flex items-start justify-between gap-3 group"
                  aria-label={`${getCategoryAriaLabel(item.category)}：${item.title}`}
                >
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      {renderCategoryChip(item.category)}
                      <span className="text-xs text-muted truncate">{item.subtitle}</span>
                    </div>
                    <div className="text-sm font-medium text-foreground truncate group-hover:text-primary transition-colors">
                      {item.title}
                    </div>
                    <div className="text-xs text-muted flex items-center gap-1 mt-1">
                      <LuClock aria-hidden="true" />
                      <span>{formatDeadlineText(item.targetDate)}</span>
                    </div>
                  </div>
                  <LuArrowUpRight
                    className="text-muted group-hover:text-primary transition-colors mt-1 shrink-0"
                    aria-hidden="true"
                  />
                </button>
              ))
            )}
          </div>
        </div>
      </div>
    </Card>
  );
}
