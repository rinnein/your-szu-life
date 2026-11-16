import { createFileRoute } from "@tanstack/react-router";
import TimetablePage, { type TimetableTab } from "../../TimetablePage";

const TABS: TimetableTab[] = ["week", "term", "periods"];

export const Route = createFileRoute("/study/timetable")({
  head: () => ({ meta: [{ title: "课表" }] }),
  validateSearch: (search: Record<string, unknown>): { tab: TimetableTab; date?: string } => ({
    tab: TABS.includes(search.tab as TimetableTab) ? (search.tab as TimetableTab) : "week",
    date: typeof search.date === "string" ? search.date : undefined,
  }),
  component: TimetableRoute,
});

function TimetableRoute() {
  const { tab, date } = Route.useSearch();
  return <TimetablePage tab={tab} date={date} />;
}
