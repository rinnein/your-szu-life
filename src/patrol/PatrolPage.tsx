import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearch } from "@tanstack/react-router";
import { listen } from "@tauri-apps/api/event";
import { Button, Card, LoadingIndicator, TextField } from "material/react";
import { LuChevronRight, LuRotateCw, LuVideo } from "react-icons/lu";
import { Banner, bindField, Chip, toast } from "../ui";
import { PageSurface } from "../shell/PageSurface";
import { TopAppBar } from "../shell/TopAppBar";
import { AuthGate } from "../shell/AuthGate";
import { useLogicalNavigation } from "../logical-navigation";
import { Choice } from "../enrollment/shared";
import { PatrolTopActions } from "./actions";
import {
  getSnapshot,
  listCourses,
  listLessons,
  mapWithConcurrency,
  refreshSnapshot,
  weekdayOf,
  type JxxkCourse,
  type JxxkLesson,
  type JxxkSnapshot,
} from "./api";

/** 与某门课程匹配的课次摘要。 */
interface CourseEntry {
  course: JxxkCourse;
  lessons: JxxkLesson[];
}

const WEEKDAY_ITEMS = [
  { id: "all", label: "全部星期" },
  { id: "1", label: "星期一" },
  { id: "2", label: "星期二" },
  { id: "3", label: "星期三" },
  { id: "4", label: "星期四" },
  { id: "5", label: "星期五" },
  { id: "6", label: "星期六" },
  { id: "7", label: "星期日" },
];

/** 今天的星期（1=周一 … 7=周日）。 */
function todayWeekday(): string {
  const day = new Date().getDay();
  return String(day === 0 ? 7 : day);
}

/** 默认选中「今天所在」的学期；找不到时回退到第一个学期。 */
function defaultSemesterId(
  semesters: { id: string; beginDate?: string | null; endDate?: string | null }[],
): string {
  const today = new Date().toISOString().slice(0, 10);
  const current = semesters.find(
    (s) => s.beginDate && s.endDate && s.beginDate <= today && today <= s.endDate,
  );
  return current?.id ?? semesters[0]?.id ?? "";
}

function liveStateText(lesson: JxxkLesson): string {
  if (lesson.liveState === 2) return "直播中";
  if (lesson.liveState === 4) return "可看回放";
  return lesson.liveStateText || "已结束";
}

function formatLessonTime(lesson: JxxkLesson): string {
  const { startTime, endTime } = lesson;
  if (!startTime) return "时间待定";
  const date = startTime.slice(0, 10);
  const start = startTime.slice(11, 16);
  const end = endTime?.slice(11, 16) ?? "";
  return `${date} ${start}${end ? ` ~ ${end}` : ""}`;
}

/** 课次的「当天时刻」排序键（HH:MM），无时间排最后。 */
function timeOfDayKey(lesson: JxxkLesson): string {
  return lesson.startTime?.slice(11, 16) ?? "99:99";
}

export default function PatrolPage() {
  const navigation = useLogicalNavigation();
  const { semester, weekday } = useSearch({ from: "/study/patrol" });
  const [snapshot, setSnapshot] = useState<JxxkSnapshot | null>(null);
  const [entries, setEntries] = useState<CourseEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [lessonsProgress, setLessonsProgress] = useState({ done: 0, total: 0 });
  const [searchKey, setSearchKey] = useState("");
  const loadTokenRef = useRef(0);

  // 实际生效的学期：URL 参数 > 本地状态 > 默认当前学期。
  // 用本地状态驱动下拉显示，URL 回写仅作深链/刷新的锦上添花。
  const [activeSemester, setActiveSemester] = useState("");
  const activeSemesterRef = useRef("");
  // 默认过滤器：本学期 + 今天对应的星期（weekday=all 表示全部）。
  const selectedWeekday = weekday ?? todayWeekday();

  const buildUrl = useCallback(
    (next: { semester?: string; weekday?: string }) => {
      const params = new URLSearchParams();
      params.set("semester", next.semester ?? activeSemester);
      params.set("weekday", next.weekday ?? selectedWeekday);
      const query = params.toString();
      return query ? `/study/patrol?${query}` : "/study/patrol";
    },
    [activeSemester, selectedWeekday],
  );

  const load = useCallback(
    async (force: boolean, semesterId?: string, silent = false) => {
      const token = ++loadTokenRef.current;
      if (force) setRefreshing(true);
      if (!silent) {
        setLoading(true);
        setEntries([]);
      }
      setLessonsProgress({ done: 0, total: 0 });
      try {
        const snap = force ? await refreshSnapshot() : await getSnapshot();
        if (token !== loadTokenRef.current) return;
        setSnapshot(snap);
        // 生效学期优先级：已生效学期 > URL 参数 > 默认当前学期。
        const valid = (id: string) => Boolean(id) && snap.semesters.some((s) => s.id === id);
        const effectiveSemester = valid(activeSemesterRef.current)
          ? activeSemesterRef.current
          : valid(semesterId ?? "")
            ? (semesterId as string)
            : valid(semester ?? "")
              ? (semester as string)
              : defaultSemesterId(snap.semesters);
        activeSemesterRef.current = effectiveSemester;
        setActiveSemester(effectiveSemester);
        // URL 回写（best-effort）：让刷新/深链保持筛选状态。
        if ((semester ?? "") !== effectiveSemester) {
          void navigation.replace(buildUrl({ semester: effectiveSemester }));
        }
        const coursePage = await listCourses(effectiveSemester || null, null);
        if (token !== loadTokenRef.current) return;
        const targets = coursePage.content;
        setLessonsProgress({ done: 0, total: targets.length });
        const aggregated = await mapWithConcurrency(
          targets,
          4,
          async (course) => {
            const page = await listLessons(course.id, 1, 200);
            // 未开始的课次不参与展示。
            return page.content.filter((lesson) => lesson.liveState !== 1);
          },
          () => {
            if (token === loadTokenRef.current) {
              setLessonsProgress((prev) => ({ ...prev, done: prev.done + 1 }));
            }
          },
        );
        if (token !== loadTokenRef.current) return;
        const grouped: CourseEntry[] = [];
        for (const [index, lessons] of aggregated.entries()) {
          if (lessons.length === 0) continue;
          grouped.push({ course: targets[index], lessons });
        }
        setEntries(grouped);
      } catch (error) {
        if (token === loadTokenRef.current) {
          toast.danger("获取巡课数据失败", { description: String(error) });
        }
      } finally {
        if (token === loadTokenRef.current) {
          setLoading(false);
          setRefreshing(false);
        }
      }
    },
    [navigation, buildUrl, semester],
  );
  const loadRef = useRef(load);
  loadRef.current = load;

  // 后台重验证完成后（乐观缓存）静默刷新：不清 entries、不闪 loading。
  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen("jxxk-cache-changed", (event) => {
      const key = event.payload as string | undefined;
      // streams 变更只影响播放页，巡课列表不响应。
      if (!active || !key || key.startsWith("streams:")) return;
      void loadRef.current(false, activeSemesterRef.current, true);
    }).then((fn) => {
      if (active) unlisten = fn;
      else fn();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    void load(false);
    // 首次挂载时加载；学期切换由 Choice 的 onChange 显式触发。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** 星期过滤后的课程视图：按一天的时间顺序（当天最早课次）排序。 */
  const filtered = useMemo(() => {
    const weekdayNum = selectedWeekday !== "all" ? Number(selectedWeekday) : null;
    const keyword = searchKey.trim().toLowerCase();
    return entries
      .map((entry) => {
        const lessons = weekdayNum
          ? entry.lessons.filter((lesson) => weekdayOf(lesson.startTime) === weekdayNum)
          : entry.lessons;
        return { course: entry.course, lessons };
      })
      .filter((entry) => {
        if (entry.lessons.length === 0) return false;
        if (!keyword) return true;
        const haystack = [
          entry.course.name,
          entry.course.teacher ?? "",
          ...entry.lessons.map((lesson) => lesson.targetName),
        ]
          .join(" ")
          .toLowerCase();
        return haystack.includes(keyword);
      })
      .map((entry) => {
        const lessons = [...entry.lessons].sort((a, b) => {
          const liveDiff = Number(b.liveState === 2) - Number(a.liveState === 2);
          if (liveDiff !== 0) return liveDiff;
          return timeOfDayKey(a).localeCompare(timeOfDayKey(b));
        });
        return { ...entry, lessons };
      })
      .sort((a, b) => {
        // 直播中的课程置顶，其余按当天最早课次的时刻排序。
        const liveDiff =
          Number(b.lessons.some((lesson) => lesson.liveState === 2)) -
          Number(a.lessons.some((lesson) => lesson.liveState === 2));
        if (liveDiff !== 0) return liveDiff;
        return timeOfDayKey(a.lessons[0]).localeCompare(timeOfDayKey(b.lessons[0]));
      });
  }, [entries, selectedWeekday, searchKey]);

  const liveCount = entries.filter((entry) =>
    entry.lessons.some((lesson) => lesson.liveState === 2),
  ).length;

  const openCourse = (entry: CourseEntry) => {
    const semesterName = snapshot?.semesters.find((s) => s.id === activeSemester)?.name ?? "";
    void navigation.push(
      `/study/patrol/course?courseId=${encodeURIComponent(entry.course.id)}` +
        `&course=${encodeURIComponent(entry.course.name)}` +
        `&semesterName=${encodeURIComponent(semesterName)}` +
        `&teacher=${encodeURIComponent(entry.course.teacher ?? "")}`,
    );
  };

  const needsAuth = snapshot?.session === "needsAuth";
  const unavailable = snapshot?.session === "unavailable";
  const semesterItems = [
    { id: "", label: "全部学期" },
    ...(snapshot?.semesters ?? []).map((s) => ({ id: s.id, label: s.name })),
  ];

  return (
    <AuthGate>
      <PageSurface maxWidth="2xl">
        <TopAppBar
          title="监察巡课"
          description="督导巡课平台（jxxk.szu.edu.cn）的课程直播与录播回放"
          actions={<PatrolTopActions />}
        />

        {needsAuth && (
          <Banner status="warning">
            <Banner.Title>尚未获取到巡课会话</Banner.Title>
            <Banner.Description>
              {snapshot?.message || "请先在应用内完成深圳大学统一认证，然后点击刷新。"}
            </Banner.Description>
          </Banner>
        )}
        {unavailable && (
          <Banner status="warning">
            <Banner.Title>巡课数据暂时不可用</Banner.Title>
            <Banner.Description>
              {snapshot?.message || "网络异常，稍后可点击刷新重试。"}
            </Banner.Description>
          </Banner>
        )}

        <div className="flex flex-wrap items-center gap-3 mb-4">
          <Choice
            label="学年学期"
            value={activeSemester}
            onChange={(value) => {
              activeSemesterRef.current = value;
              setActiveSemester(value);
              void load(false);
            }}
            items={semesterItems}
          />
          <Choice
            label="星期几"
            value={selectedWeekday}
            onChange={(value) => {
              void navigation.replace(buildUrl({ weekday: value }));
            }}
            items={WEEKDAY_ITEMS}
          />
          <div className="flex items-center gap-2 ml-auto">
            <TextField
              label="搜索"
              placeholder="课程/教师"
              value={searchKey}
              {...bindField(setSearchKey)}
              style={{ width: "160px" }}
            />
            <Button
              variant="text"
              disabled={loading || refreshing}
              onClick={() => void load(true)}
              aria-label="刷新巡课数据"
            >
              {refreshing ? <LoadingIndicator size={16} /> : <LuRotateCw aria-hidden="true" />}
              刷新
            </Button>
          </div>
        </div>

        {loading || (lessonsProgress.total > 0 && lessonsProgress.done < lessonsProgress.total) ? (
          <div className="py-8 text-center text-sm text-muted flex items-center justify-center gap-2">
            <LoadingIndicator size={16} />
            {lessonsProgress.total > 0
              ? `正在拉取课次（${lessonsProgress.done}/${lessonsProgress.total} 门课程）…`
              : "正在加载…"}
          </div>
        ) : filtered.length === 0 ? (
          <div className="py-8 text-center text-sm text-muted">
            {needsAuth || unavailable
              ? "会话恢复后即可查看课程"
              : "该筛选条件下没有课程，试试调整学期或星期"}
          </div>
        ) : (
          <div className="flex flex-col gap-2.5">
            <div className="flex items-center gap-2 text-xs text-muted">
              <span>
                共 {filtered.length} 门课程
                {liveCount > 0 && (
                  <Chip size="sm" color="danger" variant="soft" className="ml-2">
                    {liveCount} 门直播中
                  </Chip>
                )}
              </span>
            </div>
            {filtered.map((entry) => {
              const liveLesson = entry.lessons.find((lesson) => lesson.liveState === 2);
              const headline = liveLesson ?? entry.lessons[0];
              return (
                <Card key={entry.course.id} variant="outlined" className="p-3">
                  <button
                    type="button"
                    onClick={() => openCourse(entry)}
                    className="w-full text-left flex items-start justify-between gap-3 group"
                    aria-label={`打开课程：${entry.course.name}`}
                  >
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 mb-1 flex-wrap">
                        {liveLesson && (
                          <Chip size="sm" color="danger" variant="soft">
                            {liveStateText(liveLesson)}
                          </Chip>
                        )}
                        <span className="text-sm font-medium text-foreground truncate group-hover:text-primary transition-colors">
                          {entry.course.name}
                        </span>
                      </div>
                      <div className="text-xs text-muted flex items-center gap-1 mt-1 flex-wrap">
                        <LuVideo aria-hidden="true" />
                        <span>{formatLessonTime(headline)}</span>
                        {headline.classroom && <span>· {headline.classroom}</span>}
                        {entry.course.teacher && <span>· {entry.course.teacher}</span>}
                      </div>
                      <div className="text-xs text-muted mt-1">
                        {entry.lessons.length} 个可看课次
                      </div>
                    </div>
                    <LuChevronRight
                      className="text-muted group-hover:text-primary transition-colors mt-1 shrink-0"
                      aria-hidden="true"
                    />
                  </button>
                </Card>
              );
            })}
          </div>
        )}
      </PageSurface>
    </AuthGate>
  );
}
