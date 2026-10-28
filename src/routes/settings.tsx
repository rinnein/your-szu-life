import { createFileRoute } from "@tanstack/react-router";
import SettingsPage from "../SettingsPage";

export type SettingsTab =
  | "profile"
  | "appearance"
  | "notifications"
  | "models"
  | "ai"
  | "mcp"
  | "transfer"
  | "shortcuts"
  | "about"
  | "debug";

export type SettingsSearch = {
  tab?: SettingsTab;
  focus?: "ocr" | "embedding";
};

export const Route = createFileRoute("/settings")({
  head: () => ({ meta: [{ title: "账户设置" }] }),
  validateSearch: (s: Record<string, unknown>): SettingsSearch => {
    const tabs: readonly string[] = [
      "profile",
      "appearance",
      "notifications",
      "models",
      "ai",
      "mcp",
      "transfer",
      "shortcuts",
      "about",
      "debug",
    ];
    const tab =
      typeof s.tab === "string" && tabs.includes(s.tab) ? (s.tab as SettingsTab) : undefined;
    const focus = s.focus === "ocr" ? "ocr" : s.focus === "embedding" ? "embedding" : undefined;
    return { tab, focus };
  },
  component: SettingsPage,
});
