import { createFileRoute } from "@tanstack/react-router";
import SignInPage from "../../signin/SignInPage";

export const Route = createFileRoute("/study/signin")({
  head: () => ({ meta: [{ title: "签到打卡" }] }),
  validateSearch: (
    search: Record<string, unknown>,
  ): { tab: "lms" | "szu"; course?: string; start?: number; open?: boolean } => ({
    tab: search.tab === "szu" ? "szu" : "lms",
    course: typeof search.course === "string" ? search.course : undefined,
    start:
      typeof search.start === "number" && Number.isFinite(search.start) ? search.start : undefined,
    // 默认解析器会把 `open=1` 的值 JSON 解析成数字 1（merge.ts 拼的深链接）；
    // 非真时返回 undefined，避免把 open=false 序列化进 URL。
    open: search.open === true || search.open === 1 || search.open === "1" ? true : undefined,
  }),
  component: SignInPageRoute,
});

function SignInPageRoute() {
  const { tab, course, start, open } = Route.useSearch();
  return <SignInPage tab={tab} course={course} start={start} open={open} />;
}
