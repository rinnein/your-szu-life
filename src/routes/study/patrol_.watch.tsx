import { createFileRoute } from "@tanstack/react-router";
import WatchPage from "../../patrol/WatchPage";

const textParam = (value: unknown) =>
  typeof value === "number" ? String(value) : typeof value === "string" ? value : undefined;

const numberParam = (value: unknown): number | undefined => {
  if (typeof value === "number" && Number.isSafeInteger(value)) return value;
  if (typeof value === "string") {
    const parsed = Number.parseInt(value, 10);
    if (Number.isSafeInteger(parsed)) return parsed;
  }
  return undefined;
};

export const Route = createFileRoute("/study/patrol_/watch")({
  head: () => ({ meta: [{ title: "巡课播放" }] }),
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
    liveState: numberParam(search.liveState),
  }),
  component: WatchPage,
});
