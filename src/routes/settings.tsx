import { createFileRoute } from "@tanstack/react-router";
import SettingsPage from "../SettingsPage";

export const Route = createFileRoute("/settings")({
  head: () => ({ meta: [{ title: "账户设置" }] }),
  component: SettingsPage,
});
