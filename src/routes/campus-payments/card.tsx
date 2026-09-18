import { createFileRoute } from "@tanstack/react-router";
import CampusPaymentsCardPage from "../../CampusPaymentsCardPage";
export const Route = createFileRoute("/campus-payments/card")({
  component: CampusPaymentsCardPage,
});
