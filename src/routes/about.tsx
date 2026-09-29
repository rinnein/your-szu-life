import { createFileRoute } from "@tanstack/react-router";
import AboutPage from "../AboutPage";

export const Route = createFileRoute("/about")({
  head: () => ({ meta: [{ title: "关于泥深" }] }),
  component: AboutPage,
});
