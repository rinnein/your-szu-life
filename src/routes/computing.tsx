import { createFileRoute } from "@tanstack/react-router";
import ComputingCollectionPage from "../ComputingCollectionPage";
export const Route = createFileRoute("/computing")({
  head: () => ({ meta: [{ title: "计软合集" }] }),
  component: ComputingCollectionPage,
});
