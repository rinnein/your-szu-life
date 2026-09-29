import { createFileRoute } from "@tanstack/react-router";
import DebugLogPage from "../DebugLogPage";

export const Route = createFileRoute("/debug")({
  head: () => ({ meta: [{ title: "诊断日志" }] }),
  component: DebugLogPage,
});
