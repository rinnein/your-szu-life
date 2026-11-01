import { createFileRoute } from "@tanstack/react-router";
import CourseLessonsPage from "../../patrol/CourseLessonsPage";

const textParam = (value: unknown) =>
  typeof value === "number" ? String(value) : typeof value === "string" ? value : undefined;

export const Route = createFileRoute("/study/patrol_/course")({
  head: () => ({ meta: [{ title: "课程选集" }] }),
  validateSearch: (
    search: Record<string, unknown>,
  ): {
    courseId: string;
    course?: string;
    semesterName?: string;
    teacher?: string;
  } => ({
    courseId: textParam(search.courseId) ?? "",
    course: textParam(search.course),
    semesterName: textParam(search.semesterName),
    teacher: textParam(search.teacher),
  }),
  component: CourseLessonsPage,
});
