import { createFileRoute } from "@tanstack/react-router";
import SettingsPage from "../SettingsPage";

export const Route = createFileRoute("/settings")({ component: SettingsPage });
