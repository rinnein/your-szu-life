import { createFileRoute } from "@tanstack/react-router";
import LmsPage from "../../lms/LmsPage";

export const Route = createFileRoute("/study/lms")({
  head: () => ({ meta: [{ title: "畅课" }] }),
  component: LmsPage,
});
