import { createFileRoute } from "@tanstack/react-router";
import CampusPaymentsPage from "../CampusPaymentsPage";

export const Route = createFileRoute("/campus-payments")({ component: CampusPaymentsPage });
