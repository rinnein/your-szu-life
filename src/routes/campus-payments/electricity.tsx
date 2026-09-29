import { createFileRoute } from "@tanstack/react-router";
import CampusPaymentsElectricityPage from "../../CampusPaymentsElectricityPage";
export const Route = createFileRoute("/campus-payments/electricity")({
  head: () => ({ meta: [{ title: "查电量" }] }),
  component: CampusPaymentsElectricityPage,
});
