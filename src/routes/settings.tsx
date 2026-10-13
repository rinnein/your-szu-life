import { createFileRoute } from "@tanstack/react-router";
import SettingsPage from "../SettingsPage";

export const Route = createFileRoute("/settings")({
  head: () => ({ meta: [{ title: "账户设置" }] }),
  validateSearch: (s: Record<string, unknown>): { focus?: "ocr" } => ({
    focus: s.focus === "ocr" ? "ocr" : undefined,
  }),
  component: SettingsPage,
});
