import { createFileRoute } from "@tanstack/react-router";
import AcademicPage from "../../enrollment/AcademicPage";
export const Route = createFileRoute("/study/academic")({
  head: () => ({ meta: [{ title: "学业完成查询" }] }),
  component: AcademicPage,
});
