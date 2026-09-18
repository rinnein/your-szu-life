import { createFileRoute } from "@tanstack/react-router";
import CampusPaymentsRechargePage from "../../CampusPaymentsRechargePage";

export const Route = createFileRoute("/campus-payments/recharge")({
  component: CampusPaymentsRechargePage,
});
