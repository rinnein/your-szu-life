import { createFileRoute, redirect } from "@tanstack/react-router";
import CatalogPage from "../../enrollment/CatalogPage";
import { legacyDestination } from "../../enrollment/navigation";
export const Route = createFileRoute("/study/enrollment/")({
  head: () => ({ meta: [{ title: "查找课程" }] }),
  validateSearch: (s: Record<string, unknown>): { keyword?: string; tab?: string } => ({
    keyword: typeof s.keyword === "string" ? s.keyword : undefined,
    tab: typeof s.tab === "string" ? s.tab : undefined,
  }),
  beforeLoad: ({ search }) => {
    const to = legacyDestination(search.tab);
    if (to && search.tab)
      throw redirect({
        href:
          to +
          "?" +
          new URLSearchParams({
            ...(search.keyword ? { keyword: search.keyword } : {}),
            ...(search.view === "cache" ? { view: "cache" } : {}),
          }).toString(),
        replace: true,
      });
  },
  component: CatalogPage,
});
