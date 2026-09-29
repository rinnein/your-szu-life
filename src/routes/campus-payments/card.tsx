import { createFileRoute } from "@tanstack/react-router";
import CampusPaymentsCardPage from "../../CampusPaymentsCardPage";
export const Route = createFileRoute("/campus-payments/card")({
  head: () => ({ meta: [{ title: "校园卡" }] }),
  component: CampusPaymentsCardPage,
});
