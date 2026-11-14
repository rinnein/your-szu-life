import { createFileRoute } from "@tanstack/react-router";
import SignInPage from "../../signin/SignInPage";

export const Route = createFileRoute("/study/signin")({
  head: () => ({ meta: [{ title: "签到打卡" }] }),
  validateSearch: (search: Record<string, unknown>): { tab: "lms" | "szu" } => ({
    tab: search.tab === "szu" ? "szu" : "lms",
  }),
  component: SignInPageRoute,
});

function SignInPageRoute() {
  const { tab } = Route.useSearch();
  return <SignInPage tab={tab} />;
}
