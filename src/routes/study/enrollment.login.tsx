import { createFileRoute } from "@tanstack/react-router";
import LoginPage from "../../enrollment/LoginPage";
export const Route = createFileRoute("/study/enrollment/login")({
  head: () => ({ meta: [{ title: "登录选课系统" }] }),
  validateSearch: (s: Record<string, unknown>): { returnTo?: string } => ({
    returnTo: typeof s.returnTo === "string" ? s.returnTo : undefined,
  }),
  component: LoginPage,
});
