import { createFileRoute } from "@tanstack/react-router";
import SettingsPage from "../SettingsPage";

/** 设置页的 tab 全集：类型、校验与页面内的顺序表都以它为准。 */
export const SETTINGS_TAB_IDS = [
  "profile",
  "appearance",
  "notifications",
  "network",
  "storage",
  "models",
  "ai",
  "mcp",
  "sync",
  "shortcuts",
  "about",
  "debug",
] as const;

export type SettingsTab = (typeof SETTINGS_TAB_IDS)[number];

export type SettingsSearch = {
  tab?: SettingsTab;
  focus?: "ocr" | "embedding";
};

const TAB_IDS: ReadonlySet<string> = new Set(SETTINGS_TAB_IDS);

export const Route = createFileRoute("/settings")({
  head: () => ({ meta: [{ title: "账户设置" }] }),
  validateSearch: (s: Record<string, unknown>): SettingsSearch => {
    const tab =
      typeof s.tab === "string" && TAB_IDS.has(s.tab) ? (s.tab as SettingsTab) : undefined;
    const focus = s.focus === "ocr" ? "ocr" : s.focus === "embedding" ? "embedding" : undefined;
    return { tab, focus };
  },
  component: SettingsPage,
});
