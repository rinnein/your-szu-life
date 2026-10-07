import { createFileRoute } from "@tanstack/react-router";
import WebvpnPage from "../enrollment/WebvpnPage";
export const Route = createFileRoute("/webvpn")({
  head: () => ({ meta: [{ title: "WebVPN" }] }),
  component: WebvpnPage,
});
