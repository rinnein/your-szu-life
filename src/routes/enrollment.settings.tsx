import { createFileRoute } from "@tanstack/react-router";
import Page from "../enrollment/SettingsPage";
export const Route = createFileRoute("/enrollment/settings")({ component: Page });
