import { createFileRoute } from "@tanstack/react-router";
import EnrollmentLayout from "../enrollment/EnrollmentLayout";
export const Route = createFileRoute("/enrollment")({
  validateSearch: (s: Record<string, unknown>): { view?: "cache" } => ({
    view: s.view === "cache" ? "cache" : undefined,
  }),
  component: EnrollmentLayout,
});
