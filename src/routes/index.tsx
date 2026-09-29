import { createFileRoute } from "@tanstack/react-router";
import App from "../App";

export const Route = createFileRoute("/")({
  head: () => ({ meta: [{ title: "工作台" }] }),
  component: App,
});
