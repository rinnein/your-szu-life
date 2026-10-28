import { createFileRoute } from "@tanstack/react-router";
import StudyPage from "../../StudyPage";

export const Route = createFileRoute("/study/")({
  head: () => ({ meta: [{ title: "学习" }] }),
  component: StudyPage,
});
