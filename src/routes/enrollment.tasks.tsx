import { createFileRoute } from "@tanstack/react-router";
import Page from "../enrollment/TasksPage";
export const Route = createFileRoute("/enrollment/tasks")({ component: Page });
