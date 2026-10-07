import { createFileRoute } from "@tanstack/react-router";
import Page from "../enrollment/SelectedPage";
export const Route = createFileRoute("/enrollment/selected")({ component: Page });
