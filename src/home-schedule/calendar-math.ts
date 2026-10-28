import type { CalendarDayCell, ScheduleCategory } from "./types";

export function formatDateKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function getDaysInMonth(year: number, month: number): number {
  return new Date(year, month + 1, 0).getDate();
}

export function getFirstDayOfMonth(year: number, month: number): number {
  return new Date(year, month, 1).getDay();
}

export function isSameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

export function generateCalendarGrid(
  year: number,
  month: number,
  selectedDateKey: string | null,
  categoryMap: Map<string, ScheduleCategory[]>,
  today: Date = new Date(),
): CalendarDayCell[] {
  const result: CalendarDayCell[] = [];
  const firstDayWeekday = getFirstDayOfMonth(year, month);
  const daysInPrevMonth = getDaysInMonth(year, month - 1);

  // 1. Previous month trailing days
  for (let i = firstDayWeekday - 1; i >= 0; i--) {
    const day = daysInPrevMonth - i;
    const date = new Date(year, month - 1, day);
    const dateKey = formatDateKey(date);
    result.push({
      date,
      day,
      dateKey,
      isCurrentMonth: false,
      isToday: isSameDay(date, today),
      isSelected: selectedDateKey === dateKey,
      categories: categoryMap.get(dateKey) ?? [],
    });
  }

  // 2. Current month days
  const daysInCurrentMonth = getDaysInMonth(year, month);
  for (let day = 1; day <= daysInCurrentMonth; day++) {
    const date = new Date(year, month, day);
    const dateKey = formatDateKey(date);
    result.push({
      date,
      day,
      dateKey,
      isCurrentMonth: true,
      isToday: isSameDay(date, today),
      isSelected: selectedDateKey === dateKey,
      categories: categoryMap.get(dateKey) ?? [],
    });
  }

  // 3. Next month leading days to complete 42 cells (6 rows x 7 cols)
  const remainingCells = 42 - result.length;
  for (let day = 1; day <= remainingCells; day++) {
    const date = new Date(year, month + 1, day);
    const dateKey = formatDateKey(date);
    result.push({
      date,
      day,
      dateKey,
      isCurrentMonth: false,
      isToday: isSameDay(date, today),
      isSelected: selectedDateKey === dateKey,
      categories: categoryMap.get(dateKey) ?? [],
    });
  }

  return result;
}
