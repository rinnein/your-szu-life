import {
  FaCalendarDays,
  FaGraduationCap,
  FaCirclePlay,
  FaWifi,
  FaWallet,
  FaGear,
  FaCircleInfo,
  FaClipboardList,
  FaBug,
} from "react-icons/fa6";
import { LuZap, LuCode, LuCreditCard } from "react-icons/lu";
import type { CommandItem } from "./types";

export function getLastVisitedOjGroupUrl(): string {
  try {
    const raw = localStorage.getItem("oj_last_visited");
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed.siteId && parsed.groupId) {
        return `/oj?site=${encodeURIComponent(parsed.siteId)}&group=${encodeURIComponent(parsed.groupId)}`;
      }
    }
  } catch {
    // fallback
  }
  return "/oj";
}

export const COMMAND_LIST: CommandItem[] = [
  {
    id: "time",
    name: "上课时间表",
    shortcuts: ["time"],
    description: "查看各节次上课时间与作息安排",
    category: "学习",
    target: "/class-timetable",
    icon: FaCalendarDays,
    keywords: ["sk", "sksjb", "kechengbiao", "schedule", "timetable"],
  },
  {
    id: "oj",
    name: "OJ首页",
    shortcuts: ["oj"],
    description: "在线评测系统（OJ）首页与题库列表",
    category: "学习",
    target: "/oj",
    icon: LuCode,
    keywords: ["online judge", "ceyan", "tiku", "programming"],
  },
  {
    id: "ojzj",
    name: "最近测验列表",
    shortcuts: ["ojzj"],
    description: "最后一次打开的OJ的最后一次打开的班级的测验列表",
    category: "学习",
    target: getLastVisitedOjGroupUrl,
    icon: LuCode,
    keywords: ["zuijin", "banji", "zj", "recent contest"],
  },
  {
    id: "uooc",
    name: "UOOC任务",
    shortcuts: ["uooc", "mooc"],
    description: "优课在线视频学习与平时任务",
    category: "学习",
    target: "/uooc?tab=tasks",
    icon: FaCirclePlay,
    keywords: ["youke", "shuake", "renwu", "video"],
  },
  {
    id: "net",
    name: "校园网登录",
    shortcuts: ["net", "xyw"],
    description: "教工区与宿舍区校园网认证与登录",
    category: "生活",
    target: "/campus-network",
    icon: FaWifi,
    keywords: ["wangluo", "denglu", "wifi", "network", "portal"],
  },
  {
    id: "card",
    name: "校园卡",
    shortcuts: ["card", "xyk"],
    description: "校园卡充值与明细查询",
    category: "生活",
    target: "/campus-payments/card",
    icon: LuCreditCard,
    keywords: ["yikatong", "chongzhi", "yue", "card balance"],
  },
  {
    id: "cdl",
    name: "查电量",
    shortcuts: ["cdl"],
    description: "宿舍剩余电量与用电明细查询",
    category: "生活",
    target: "/campus-payments/electricity",
    icon: LuZap,
    keywords: ["dianfei", "dianliang", "chadian", "power", "electricity"],
  },
  {
    id: "cdf",
    name: "充电费",
    shortcuts: ["cdf"],
    description: "通过官方微信服务办理宿舍电费充值",
    category: "生活",
    target: "/campus-payments/recharge",
    icon: FaWallet,
    keywords: ["chongdian", "chongzhi", "recharge", "pay electricity"],
  },
  {
    id: "settings",
    name: "设置",
    shortcuts: ["sz", "set", "settings"],
    description: "管理账户资料、本地模型与应用设置",
    category: "系统",
    target: "/settings",
    icon: FaGear,
    keywords: ["shezhi", "config", "preferences"],
  },
  {
    id: "enrollment",
    name: "选课",
    shortcuts: ["xk", "enrollment"],
    description: "查课、抢课与选课期间课表",
    category: "学习",
    target: "/enrollment",
    icon: FaClipboardList,
    keywords: ["xuanke", "qiangke", "course"],
  },
  {
    id: "academic",
    name: "学业完成查询",
    shortcuts: ["xy", "academic"],
    description: "培养方案、学分与课程完成情况",
    category: "学习",
    target: "/academic",
    icon: FaGraduationCap,
    keywords: ["xueye", "xuefen", "degree"],
  },
  {
    id: "webvpn",
    name: "WebVPN",
    shortcuts: ["vpn", "webvpn"],
    description: "校外访问与专属2FA认证",
    category: "其它",
    target: "/webvpn",
    icon: FaWifi,
    keywords: ["xiaowai", "fangwen"],
  },
  {
    id: "about",
    name: "关于",
    shortcuts: ["about", "gy"],
    description: "应用信息与检查更新",
    category: "系统",
    target: "/about",
    icon: FaCircleInfo,
    keywords: ["guanyu", "gengxin", "version", "info"],
  },
  {
    id: "debug",
    name: "诊断日志",
    shortcuts: ["debug", "log"],
    description: "查看网络与运行时诊断记录",
    category: "系统",
    target: "/debug",
    icon: FaBug,
    keywords: ["rizhi", "zhenduan"],
  },
];

export function filterCommands(query: string, commands: CommandItem[] = COMMAND_LIST): CommandItem[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return commands;

  return commands
    .map((cmd) => {
      let score = 0;

      // Exact shortcut match gets highest priority
      if (cmd.shortcuts.some((s) => s.toLowerCase() === normalized)) {
        score += 100;
      } else if (cmd.shortcuts.some((s) => s.toLowerCase().startsWith(normalized))) {
        score += 60;
      } else if (cmd.shortcuts.some((s) => s.toLowerCase().includes(normalized))) {
        score += 30;
      }

      // Name match
      const nameLower = cmd.name.toLowerCase();
      if (nameLower === normalized) {
        score += 80;
      } else if (nameLower.startsWith(normalized)) {
        score += 50;
      } else if (nameLower.includes(normalized)) {
        score += 25;
      }

      // Description match
      if (cmd.description.toLowerCase().includes(normalized)) {
        score += 10;
      }

      // Keywords match
      if (cmd.keywords?.some((k) => k.toLowerCase().includes(normalized))) {
        score += 15;
      }

      return { cmd, score };
    })
    .filter((entry) => entry.score > 0)
    .sort((a, b) => b.score - a.score)
    .map((entry) => entry.cmd);
}
