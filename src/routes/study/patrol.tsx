import { createFileRoute } from "@tanstack/react-router";
import PatrolPage from "../../patrol/PatrolPage";

const textParam = (value: unknown) =>
  typeof value === "number" ? String(value) : typeof value === "string" ? value : undefined;

export const Route = createFileRoute("/study/patrol")({
  head: () => ({ meta: [{ title: "监察巡课" }] }),
  validateSearch: (
    search: Record<string, unknown>,
  ): {
    semester?: string;
    weekday?: string;
    course?: string;
  } => ({
    semester: textParam(search.semester),
    weekday: textParam(search.weekday),
    course: textParam(search.course),
  }),
  component: PatrolPage,
});
