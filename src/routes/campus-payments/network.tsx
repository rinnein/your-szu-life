import { createFileRoute } from "@tanstack/react-router";
import CampusPaymentsNetworkPage from "../../CampusPaymentsNetworkPage";
export const Route = createFileRoute("/campus-payments/network")({
  component: CampusPaymentsNetworkPage,
});
