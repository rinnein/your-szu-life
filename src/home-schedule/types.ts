export type ScheduleCategory =
  | "oj-quiz"
  | "electricity-outage"
  | "uooc-exam"
  | "uooc-homework"
  | "uooc-course-end"
  | "lms-homework";

export interface ScheduleItem {
  id: string;
  category: ScheduleCategory;
  title: string;
  subtitle?: string;
  targetDate: Date;
  dateKey: string; // "YYYY-MM-DD"
  actionUrl: string;
  /** 外部链接（如畅课任务页）：桌面端点击直接在浏览器打开，Android 弹链接模态框。 */
  externalUrl?: string;
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
