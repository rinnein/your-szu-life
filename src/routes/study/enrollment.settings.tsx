import { createFileRoute } from "@tanstack/react-router";
import Page from "../../enrollment/SettingsPage";
export const Route = createFileRoute("/study/enrollment/settings")({ component: Page });
