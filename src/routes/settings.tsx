import { createFileRoute } from "@tanstack/react-router";
import SettingsPage from "../SettingsPage";

export type SettingsTab = "profile" | "models" | "ai" | "transfer";

export type SettingsSearch = {
  tab?: SettingsTab;
  focus?: "ocr" | "embedding";
};

export const Route = createFileRoute("/settings")({
  head: () => ({ meta: [{ title: "账户设置" }] }),
  validateSearch: (s: Record<string, unknown>): SettingsSearch => {
    const tab =
      s.tab === "profile" || s.tab === "models" || s.tab === "ai" || s.tab === "transfer"
        ? s.tab
        : undefined;
    const focus = s.focus === "ocr" ? "ocr" : s.focus === "embedding" ? "embedding" : undefined;
    return { tab, focus };
  },
  component: SettingsPage,
});
