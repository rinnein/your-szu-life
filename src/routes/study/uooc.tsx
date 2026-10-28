import { createFileRoute } from "@tanstack/react-router";
import UoocPage from "../../uooc/UoocPage";

export const Route = createFileRoute("/study/uooc")({
  head: () => ({ meta: [{ title: "UOOC刷课" }] }),
  validateSearch: (
    search: Record<string, unknown>,
  ): {
    tab: "courses" | "tasks" | "exams" | "qa" | "settings";
    subtab?: "quiz" | "homework" | "exam";
    course?: string;
  } => ({
    course:
      typeof search.course === "string" && /^\d+$/.test(search.course) ? search.course : undefined,
    tab:
      search.tab === "tasks" ||
      search.tab === "exams" ||
      search.tab === "qa" ||
      search.tab === "settings"
        ? search.tab
        : "courses",
    subtab:
      search.subtab === "quiz" || search.subtab === "homework" || search.subtab === "exam"
        ? search.subtab
        : undefined,
  }),
  component: UoocPage,
});
