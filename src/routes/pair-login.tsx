import { createFileRoute } from "@tanstack/react-router";
import { PairAuthorizePage } from "../pair-login/AuthorizePage";

export const Route = createFileRoute("/pair-login")({
  head: () => ({ meta: [{ title: "授权登录" }] }),
  component: PairAuthorizePage,
});
