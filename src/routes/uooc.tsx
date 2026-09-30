import { createFileRoute } from "@tanstack/react-router";
import UoocPage from "../uooc/UoocPage";

export const Route = createFileRoute("/uooc")({
  head: () => ({ meta: [{ title: "UOOC刷课" }] }),
  validateSearch: (search: Record<string, unknown>): { tab: "courses" | "tasks" | "settings" } => ({
    tab: search.tab === "tasks" || search.tab === "settings" ? search.tab : "courses",
  }),
  component: UoocPage,
});
