import { createFileRoute } from "@tanstack/react-router";
import DebugLogPage from "../DebugLogPage";

export const Route = createFileRoute("/debug")({ component: DebugLogPage });
