import { createFileRoute } from "@tanstack/react-router";
import HomePage from "../HomePage";

export const Route = createFileRoute("/")({
  head: () => ({ meta: [{ title: "首页" }] }),
  component: HomePage,
});
