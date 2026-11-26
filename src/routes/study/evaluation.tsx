import { createFileRoute } from "@tanstack/react-router";
import EvaluationPage from "../../evaluation/EvaluationPage";

export const Route = createFileRoute("/study/evaluation")({
  head: () => ({ meta: [{ title: "教学评价" }] }),
  component: EvaluationPage,
});
