import { createFileRoute } from "@tanstack/react-router";
import Page from "../../enrollment/TasksPage";
export const Route = createFileRoute("/study/enrollment/tasks")({ component: Page });
