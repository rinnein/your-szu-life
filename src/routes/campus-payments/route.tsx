import { createFileRoute } from "@tanstack/react-router";
import CampusPaymentsPage from "../../CampusPaymentsPage";
export const Route = createFileRoute("/campus-payments")({
  head: () => ({ meta: [{ title: "校园卡/电费/网费" }] }),
  component: CampusPaymentsPage,
});
