import { useLogicalNavigation } from "../../logical-navigation";
import { createFileRoute } from "@tanstack/react-router";
import { useEffect } from "react";
import { CAMPUS_PAYMENTS_ROUTES, readLastCampusPaymentsRoute } from "../../campus-payments";
export const Route = createFileRoute("/campus-payments/")({
  component: () => {
    const navigation = useLogicalNavigation();
    useEffect(() => {
      const saved = readLastCampusPaymentsRoute();
      const target = (CAMPUS_PAYMENTS_ROUTES as readonly string[]).includes(saved ?? "")
        ? saved!
        : "/campus-payments/electricity";
      void navigation.replace(target as "/campus-payments/electricity");
    }, [navigation]);
    return null;
  },
});
