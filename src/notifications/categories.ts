import type { IconType } from "react-icons";
import {
  LuBookOpen,
  LuClipboardList,
  LuCode,
  LuFileText,
  LuGraduationCap,
  LuWifi,
  LuZap,
} from "react-icons/lu";

/**
 * 通知分类的唯一事实来源。
 *
 * 「设置 → 通知」的分类开关与应用内通知弹窗都从这里取标题、图标与默认级别，
 * 避免两处各维护一份分类表后逐渐走偏。
 *
 * 待办类通知不在这个扁平列表里：它由下面的 `TODO_NOTIFICATION_ITEMS`
 * 按注册项单独管理，避免与 OJ/UOOC 的非截止通知混在一起。
 */
export type NotificationCategory =
  | "campus_network"
  | "electricity"
  | "enrollment"
  | "oj"
  | "uooc"
  | "todo";

export type NotificationConfigKey = "campusNetwork" | "electricity" | "enrollment" | "oj" | "uooc";

export interface NotificationCategoryMeta {
  category: NotificationCategory;
  configKey: NotificationConfigKey;
  title: string;
  description: string;
  icon: IconType;
  /** 该类通知的默认提示级别；正文命中失败/不足等字样时仍会升级为 warning。 */
  severity: "info" | "warning";
}

export const NOTIFICATION_CATEGORIES: readonly NotificationCategoryMeta[] = [
  {
    category: "campus_network",
    configKey: "campusNetwork",
    title: "宿舍校园网",
    description: "自动登录失败或断线重连异常时发送通知",
    icon: LuWifi,
    severity: "warning",
  },
  {
    category: "electricity",
    configKey: "electricity",
    title: "宿舍电费预警",
    description: "宿舍电量偏低或预计可用天数不足时发送充值预警",
    icon: LuZap,
    severity: "warning",
  },
  {
    category: "enrollment",
    configKey: "enrollment",
    title: "选课助手",
    description: "抢课任务完成、暂停或需要 2FA 二次验证时发送通知",
    icon: LuGraduationCap,
    severity: "info",
  },
  {
    category: "oj",
    configKey: "oj",
    title: "在线评测 (OJ)",
    description: "新测验发布与 OJ 登录状态提醒（截止提醒见「待办提醒」）",
    icon: LuCode,
    severity: "info",
  },
  {
    category: "uooc",
    configKey: "uooc",
    title: "优课联盟 (UOOC)",
    description: "UOOC 测验需要处理时发送提醒（截止提醒见「待办提醒」）",
    icon: LuBookOpen,
    severity: "info",
  },
];

/** 待办提醒的六个注册项，与后端 `TodoKind` 及首页日历分类一一对应。 */
export type TodoConfigKey =
  | "ojQuiz"
  | "uoocExam"
  | "uoocHomework"
  | "uoocCourseEnd"
  | "lmsHomework"
  | "electricityOutage";

export interface TodoNotificationItemMeta {
  key: TodoConfigKey;
  title: string;
  description: string;
  icon: IconType;
}

export const TODO_NOTIFICATION_ITEMS: readonly TodoNotificationItemMeta[] = [
  {
    key: "ojQuiz",
    title: "OJ 测验",
    description: "OJ 测验/考试截止前提醒",
    icon: LuCode,
  },
  {
    key: "uoocExam",
    title: "UOOC 考试",
    description: "UOOC 考试截止前提醒",
    icon: LuBookOpen,
  },
  {
    key: "uoocHomework",
    title: "UOOC 作业",
    description: "UOOC 作业截止前提醒",
    icon: LuFileText,
  },
  {
    key: "uoocCourseEnd",
    title: "UOOC 结课",
    description: "UOOC 课程结课日期提醒",
    icon: LuGraduationCap,
  },
  {
    key: "lmsHomework",
    title: "畅课作业",
    description: "畅课课程待办截止前提醒",
    icon: LuClipboardList,
  },
  {
    key: "electricityOutage",
    title: "电费预估断电",
    description: "宿舍电量预计耗尽日期提醒",
    icon: LuZap,
  },
];

/** 提前提醒时间预设；后端存储为分钟数组，允许用户勾选其中任意多项。 */
export const TODO_LEAD_PRESETS: readonly { minutes: number; label: string }[] = [
  { minutes: 7 * 24 * 60, label: "7 天前" },
  { minutes: 3 * 24 * 60, label: "3 天前" },
  { minutes: 2 * 24 * 60, label: "2 天前" },
  { minutes: 24 * 60, label: "1 天前" },
  { minutes: 2 * 60, label: "2 小时前" },
  { minutes: 60, label: "1 小时前" },
  { minutes: 30, label: "30 分钟前" },
  { minutes: 15, label: "15 分钟前" },
  { minutes: 5, label: "5 分钟前" },
];

/** 默认勾选的档位：7天、3天、1天、2小时。 */
export const DEFAULT_TODO_LEAD_MINUTES: readonly number[] = [
  7 * 24 * 60,
  3 * 24 * 60,
  24 * 60,
  2 * 60,
];

const CATEGORY_META = new Map<string, NotificationCategoryMeta>(
  NOTIFICATION_CATEGORIES.map((item) => [item.category, item]),
);

/** 后端已按分类开关过滤，这里只决定前台 Snackbar 用 info 还是 warning。 */
export function notificationSeverity(category: string, title = "", body = ""): "info" | "warning" {
  if (CATEGORY_META.get(category)?.severity === "warning") return "warning";
  return /失败|不足/.test(`${title}${body}`) ? "warning" : "info";
}
