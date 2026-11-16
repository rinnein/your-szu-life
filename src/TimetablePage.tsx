import { useMemo } from "react";
import { LoadingIndicator, Button } from "material/react";
import { AdaptiveTabs, Banner } from "./ui";
import { useLogicalNavigation } from "./logical-navigation";
import { PageSurface } from "./shell/PageSurface";
import { TopAppBar } from "./shell/TopAppBar";
import { AuthGate } from "./shell/AuthGate";
import { useTimetable } from "./timetable/use-timetable";
import { WeekGrid } from "./timetable/WeekGrid";
import { TermGrid } from "./timetable/TermGrid";
import { PeriodTable } from "./timetable/PeriodTable";
import { dateFromKey } from "./timetable/merge";

export type TimetableTab = "week" | "term" | "periods";

const TABS = [
  { value: "week", label: "本周" },
  { value: "term", label: "学期" },
  { value: "periods", label: "作息" },
] as const;

export default function TimetablePage({ tab, date }: { tab: TimetableTab; date?: string }) {
  const navigation = useLogicalNavigation();
  const { snapshot, error, refresh, refreshing } = useTimetable();
  const initialDate = useMemo(() => dateFromKey(date), [date]);

  return (
    <AuthGate>
      <PageSurface maxWidth="2xl">
        <TopAppBar
          title="课表"
          description="本周课程、学期课表、待签提醒与作息时间"
          actions={
            <Button
              variant="text"
              disabled={refreshing}
              onClick={() => void refresh({ forceSemester: true })}
              aria-label="刷新课表"
            >
              {refreshing ? <LoadingIndicator size={16} /> : null}
              刷新
            </Button>
          }
        />

        <AdaptiveTabs
          label="课表视图"
          items={TABS.map((item) => ({ value: item.value, label: item.label }))}
          value={tab}
          onChange={(next) => void navigation.replace(`/study/timetable?tab=${next}`)}
          ariaLabel="课表视图"
        />

        {(error || snapshot?.message) && (
          <Banner status="warning">
            <Banner.Title>课表暂不可用</Banner.Title>
            <Banner.Description>{error ?? snapshot?.message}</Banner.Description>
          </Banner>
        )}

        {!snapshot ? (
          <div className="grid justify-items-center gap-3 py-8 text-sm text-muted">
            <LoadingIndicator size={24} />
            <span>正在加载课表…</span>
          </div>
        ) : tab === "week" ? (
          <WeekGrid
            sessions={snapshot.sessions}
            semesterCourses={snapshot.semesterCourses}
            initialDate={initialDate}
          />
        ) : tab === "term" ? (
          <TermGrid
            courses={snapshot.semesterCourses}
            message={snapshot.semesterMessage}
            fetchedAtMs={snapshot.semesterFetchedAtMs}
          />
        ) : (
          <PeriodTable />
        )}
      </PageSurface>
    </AuthGate>
  );
}
