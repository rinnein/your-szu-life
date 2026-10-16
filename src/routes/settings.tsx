import { createFileRoute } from "@tanstack/react-router";
import SettingsPage from "../SettingsPage";

export const Route = createFileRoute("/settings")({
  head: () => ({ meta: [{ title: "账户设置" }] }),
  validateSearch: (s: Record<string, unknown>): { focus?: "ocr" | "embedding" } => ({
    focus: s.focus === "ocr" ? "ocr" : s.focus === "embedding" ? "embedding" : undefined,
  }),
  component: SettingsPage,
});
