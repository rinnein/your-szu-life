import type { JxxkSnapshot } from "../../patrol/api";

export const mockJxxkSnapshot: JxxkSnapshot = {
  session: "ready",
  message: "",
  semesters: [
    {
      id: "2026-1",
      name: "2026-2027学年第一学期",
      beginDate: "2026-09-07",
      endDate: "2027-01-10",
    },
  ],
  fetchedAt: Date.now() - 120_000,
  refreshing: false,
};
