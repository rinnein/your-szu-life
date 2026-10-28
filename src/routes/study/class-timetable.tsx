import { createFileRoute } from "@tanstack/react-router";
import ClassTimetablePage from "../../ClassTimetablePage";

export const Route = createFileRoute("/study/class-timetable")({
  head: () => ({ meta: [{ title: "上课时间表" }] }),
  component: ClassTimetablePage,
});
