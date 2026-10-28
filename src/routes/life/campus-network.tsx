import { createFileRoute } from "@tanstack/react-router";
import CampusNetworkPage from "../../CampusNetworkPage";

export const Route = createFileRoute("/life/campus-network")({
  head: () => ({ meta: [{ title: "校园网" }] }),
  component: CampusNetworkPage,
});
