export type SessionCheckin = "none" | "pending_lms" | "pending_szu" | "signed_lms" | "signed_szu";

export type TimetableSession = {
  date: string;
  courseName: string;
  buildingName?: string | null;
  buildingNameEn?: string | null;
  classroom?: string | null;
  startMs: number;
  endMs: number;
  semester?: string | null;
  checkin: SessionCheckin;
};

export type PendingCheckin = {
  id: string;
  title: string;
  subtitle?: string | null;
  firstSeenMs: number;
  route: string;
};

/** eHall 固定学期课表中的一个排课槽。 */
export type SemesterCourse = {
  name: string;
  teacher: string;
  classroom: string;
  /** 1-7。 */
  weekday: number;
  /** 1-14。 */
  startPeriod: number;
  endPeriod: number;
  weeks: number[];
  rawWeeks: string;
};

export type TimetableSnapshot = {
  fetchedAtMs: number;
  semester?: string | null;
  message: string;
  sessions: TimetableSession[];
  checkins: PendingCheckin[];
  semesterCourses: SemesterCourse[];
  semesterFetchedAtMs: number;
  semesterMessage: string;
};

export type TimetableEvent = {
  type: "initial" | "snapshot";
  data: TimetableSnapshot;
};
