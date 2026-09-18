import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect } from "react";
import { CAMPUS_PAYMENTS_ROUTES, readLastCampusPaymentsRoute } from "../../campus-payments";
export const Route = createFileRoute("/campus-payments/")({
  component: () => {
    const navigate = useNavigate();
    useEffect(() => {
      const saved = readLastCampusPaymentsRoute();
      const target = (CAMPUS_PAYMENTS_ROUTES as readonly string[]).includes(saved ?? "")
        ? saved!
        : "/campus-payments/electricity";
      void navigate({ to: target as "/campus-payments/electricity", replace: true });
    }, [navigate]);
    return null;
  },
});
