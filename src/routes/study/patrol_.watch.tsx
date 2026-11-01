import { createFileRoute } from "@tanstack/react-router";
import WatchPage from "../../patrol/WatchPage";

const textParam = (value: unknown) =>
  typeof value === "number" ? String(value) : typeof value === "string" ? value : undefined;

export const Route = createFileRoute("/study/patrol_/watch")({
  head: () => ({ meta: [{ title: "巡课回放" }] }),
  validateSearch: (
    search: Record<string, unknown>,
  ): {
    courseId: string;
    liveId: string;
    course?: string;
    semesterName?: string;
    teacher?: string;
    title?: string;
    liveState?: number;
  } => ({
    courseId: textParam(search.courseId) ?? "",
    liveId: textParam(search.liveId) ?? "",
    course: textParam(search.course),
    semesterName: textParam(search.semesterName),
    teacher: textParam(search.teacher),
    title: textParam(search.title),
    liveState:
      typeof search.liveState === "number" && Number.isSafeInteger(search.liveState)
        ? search.liveState
        : undefined,
  }),
  component: WatchPage,
});
