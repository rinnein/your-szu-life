import { createFileRoute } from "@tanstack/react-router";
import Page from "../enrollment/TimetablePage";
export const Route = createFileRoute("/enrollment/timetable")({ component: Page });
