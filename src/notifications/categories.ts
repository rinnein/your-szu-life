import type { IconType } from "react-icons";
import { LuBookOpen, LuCode, LuGraduationCap, LuWifi, LuZap } from "react-icons/lu";

/**
 * 通知分类的唯一事实来源。
 *
 * 「设置 → 通知」的分类开关与应用内通知弹窗都从这里取标题、图标与默认级别，
 * 避免两处各维护一份分类表后逐渐走偏。
 */
export type NotificationCategory = "campus_network" | "electricity" | "enrollment" | "oj" | "uooc";

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
    description: "新测验发布及测验临近截止（1小时前）提醒",
    icon: LuCode,
    severity: "info",
  },
  {
    category: "uooc",
    configKey: "uooc",
    title: "优课联盟 (UOOC)",
    description: "课程测验、作业待处理或临近截止时发送提醒",
    icon: LuBookOpen,
    severity: "info",
  },
];

const CATEGORY_META = new Map<string, NotificationCategoryMeta>(
  NOTIFICATION_CATEGORIES.map((item) => [item.category, item]),
);

/** 后端已按分类开关过滤，这里只决定前台 Snackbar 用 info 还是 warning。 */
export function notificationSeverity(category: string, title = "", body = ""): "info" | "warning" {
  if (CATEGORY_META.get(category)?.severity === "warning") return "warning";
  return /失败|不足/.test(`${title}${body}`) ? "warning" : "info";
}
