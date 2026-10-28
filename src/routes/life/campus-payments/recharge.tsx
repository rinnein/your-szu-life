import { createFileRoute } from "@tanstack/react-router";
import CampusPaymentsRechargePage from "../../../CampusPaymentsRechargePage";

export const Route = createFileRoute("/life/campus-payments/recharge")({
  head: () => ({ meta: [{ title: "充电费" }] }),
  component: CampusPaymentsRechargePage,
});
