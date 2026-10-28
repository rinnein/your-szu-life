import type { IconType } from "react-icons";

export interface CommandItem {
  id: string;
  name: string;
  shortcuts: string[]; // 英文缩写列表
  description: string;
  category: "导航" | "学习" | "生活" | "系统";
  target: string | (() => string);
  icon?: IconType;
  keywords?: string[];
}

export interface CommandUsage {
  id: string;
  useCount: number;
  lastUsedAt: number;
}
