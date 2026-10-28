import { createFileRoute } from "@tanstack/react-router";
import LifePage from "../../LifePage";

export const Route = createFileRoute("/life/")({
  head: () => ({ meta: [{ title: "生活" }] }),
  component: LifePage,
});
