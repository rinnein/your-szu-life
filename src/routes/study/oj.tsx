import { createFileRoute } from "@tanstack/react-router";
import OjPage from "../../oj/OjPage";
const numericId = (value: unknown) => {
  const text = typeof value === "number" && Number.isSafeInteger(value) ? String(value) : value;
  return typeof text === "string" && /^\d+$/.test(text) ? text : undefined;
};
export const Route = createFileRoute("/study/oj")({
  head: () => ({ meta: [{ title: "OJ · 编程练习" }] }),
  validateSearch: (
    search: Record<string, unknown>,
  ): { site?: string; group?: string; contest?: string; problem?: string; tab: string } => ({
    site:
      typeof search.site === "string" && /^[a-zA-Z0-9-]{1,64}$/.test(search.site)
        ? search.site
        : undefined,
    group: numericId(search.group),
    contest: numericId(search.contest),
    problem:
      typeof search.problem === "string" && /^[0-4]:\d+$/.test(search.problem)
        ? search.problem
        : undefined,
    tab: search.tab === "answer" || search.tab === "records" ? search.tab : "statement",
  }),
  component: OjPage,
});
