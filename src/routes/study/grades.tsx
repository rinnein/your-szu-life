import { createFileRoute } from "@tanstack/react-router";
import GradesPage from "../../enrollment/GradesPage";
export const Route = createFileRoute("/study/grades")({
  head: () => ({ meta: [{ title: "成绩查询" }] }),
  component: GradesPage,
});
