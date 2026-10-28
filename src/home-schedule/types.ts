export type ScheduleCategory = "oj-quiz" | "electricity-outage";

export interface ScheduleItem {
  id: string;
  category: ScheduleCategory;
  title: string;
  subtitle?: string;
  targetDate: Date;
  dateKey: string; // "YYYY-MM-DD"
  actionUrl: string;
  urgent?: boolean;
}

export interface CalendarDayCell {
  date: Date;
  day: number;
  dateKey: string; // "YYYY-MM-DD"
  isCurrentMonth: boolean;
  isToday: boolean;
  isSelected: boolean;
  categories: ScheduleCategory[];
}
