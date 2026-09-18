import { createFileRoute } from "@tanstack/react-router";
import CampusPaymentsElectricityPage from "../../CampusPaymentsElectricityPage";
export const Route = createFileRoute("/campus-payments/electricity")({
  component: CampusPaymentsElectricityPage,
});
