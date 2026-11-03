import { useCallback, useEffect, useState } from "react";
import { useSearch } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { Button, Card, LoadingIndicator } from "material/react";
import { LuMonitorPlay, LuRotateCw } from "react-icons/lu";
import { Chip, toast } from "../ui";
import { PageSurface } from "../shell/PageSurface";
import { TopAppBar } from "../shell/TopAppBar";
import { AuthGate } from "../shell/AuthGate";
import { useLogicalNavigation } from "../logical-navigation";
import { useChannelSubscription } from "../tauri-channel";
import { PatrolTopActions } from "./actions";
import { type JxxkLesson, type JxxkState } from "./api";

function liveStateColor(state: number | null | undefined): "danger" | "success" | "default" {
  switch (state) {
    case 2:
      return "danger";
    case 4:
      return "success";
    default:
      return "default";
  }
}

function liveStateText(lesson: JxxkLesson): string {
  if (lesson.liveState === 2) return lesson.liveStateText || "直播中";
  if (lesson.liveState === 4) return "可看回放";
  return lesson.liveStateText || "已结束";
}

/** 突出显示课次名中的「第N周」部分。 */
function HighlightWeek({ name }: { name: string }) {
  const parts = name.split(/(第[0-9一二三四五六七八九十百]+周)/g);
  return (
    <>
      {parts.map((part, index) =>
        /^第[0-9一二三四五六七八九十百]+周$/.test(part) ? (
          <span key={index} className="text-primary font-semibold">
            {part}
          </span>
        ) : (
          <span key={index}>{part}</span>
        ),
      )}
    </>
  );
}

/** 卡片左下角时间：日期部分突出显示。 */
function LessonTime({ lesson }: { lesson: JxxkLesson }) {
  const { startTime, endTime } = lesson;
  if (!startTime) return <span>时间待定</span>;
  const date = startTime.slice(0, 10);
  const start = startTime.slice(11, 16);
  const end = endTime?.slice(11, 16) ?? "";
  return (
    <span>
      <span className="text-foreground font-medium">{date}</span> {start}
      {end ? ` ~ ${end}` : ""}
    </span>
  );
}

/** 课程的选集页：按时间顺序列出这门课可观看的直播与录播。 */
export default function CourseLessonsPage() {
  const navigation = useLogicalNavigation();
  const { courseId, course, semesterName, teacher } = useSearch({
    from: "/study/patrol_/course",
  });
  const [lessons, setLessons] = useState<JxxkLesson[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [backendRefreshing, setBackendRefreshing] = useState(false);

  const load = useCallback(
    async (force: boolean) => {
      if (force) setRefreshing(true);
      else setLoading(true);
      try {
        const page = await invoke<{ content: JxxkLesson[] }>("jxxk_list_lessons", {
          courseId,
          pageIndex: 1,
          pageSize: 200,
        });
        // 未开始的课次不展示；按开始时间先后排成选集。
        setLessons(
          page.content
            .filter((lesson) => lesson.liveState !== 1)
            // 时间倒序：越晚的课次越靠前。
            .sort((a, b) => (b.startTime ?? "").localeCompare(a.startTime ?? "")),
        );
      } catch (error) {
        toast.danger("获取课程课次失败", { description: String(error) });
      } finally {
        setLoading(false);
        setRefreshing(false);
      }
    },
    // force 刷新走同一命令但保留旧内容展示。
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [courseId],
  );

  useEffect(() => {
    void load(false);
  }, [load]);

  useChannelSubscription<{ type: "initial" | "snapshot"; data: JxxkState }>(
    "subscribe_jxxk_state",
    (event) => {
      const data = event?.data;
      if (!data) return;
      const isRevalidating =
        data.snapshot?.refreshing === true ||
        Boolean(
          data.revalidatingKeys?.some((k) => k === `lessons:${courseId}` || k === "semesters"),
        );
      setBackendRefreshing(isRevalidating);
      const page = data.lessons?.[courseId];
      if (page?.content) {
        setLessons(
          page.content
            .filter((lesson) => lesson.liveState !== 1)
            .sort((a, b) => (b.startTime ?? "").localeCompare(a.startTime ?? "")),
        );
      }
    },
  );

  const isRefreshing = refreshing || backendRefreshing;

  const openLesson = (lesson: JxxkLesson) => {
    const state = lesson.liveState;
    if (!lesson.liveId || !state) {
      toast.info("该课次暂不可观看");
      return;
    }
    if (state === 3) {
      toast.info("课堂录像生成中", {
        description: "课程已结束，回放通常在课后一小时内可观看。",
      });
      return;
    }
    void navigation.push(
      `/study/patrol/watch?courseId=${encodeURIComponent(courseId)}` +
        `&liveId=${encodeURIComponent(lesson.liveId)}` +
        `&course=${encodeURIComponent(course ?? "")}` +
        `&semesterName=${encodeURIComponent(semesterName ?? "")}` +
        `&teacher=${encodeURIComponent(teacher ?? "")}` +
        `&title=${encodeURIComponent(lesson.targetName)}` +
        `&liveState=${state}`,
    );
  };

  return (
    <AuthGate>
      <PageSurface maxWidth="2xl">
        <TopAppBar
          title={course || "课程选集"}
          description="按授课时间排列的直播与录播"
          actions={<PatrolTopActions />}
        />

        {loading ? (
          <div className="py-8 text-center text-sm text-muted flex items-center justify-center gap-2">
            <LoadingIndicator size={16} /> 正在加载…
          </div>
        ) : lessons.length === 0 ? (
          <div className="py-8 text-center text-sm text-muted">这门课暂无可观看的直播或回放</div>
        ) : (
          <div className="flex flex-col gap-2.5">
            <div className="flex items-center justify-between gap-3 text-xs text-muted">
              <span>共 {lessons.length} 个课次</span>
              <Button
                variant="text"
                disabled={isRefreshing}
                onClick={() => void load(true)}
                aria-label="刷新课次列表"
              >
                {isRefreshing ? <LoadingIndicator size={16} /> : <LuRotateCw aria-hidden="true" />}
                刷新
              </Button>
            </div>
            {lessons.map((lesson) => (
              <Card key={lesson.id} variant="outlined" className="p-3">
                <button
                  type="button"
                  onClick={() => openLesson(lesson)}
                  className="w-full text-left flex items-start justify-between gap-3 group"
                  aria-label={`打开课次：${lesson.targetName}`}
                >
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1 flex-wrap">
                      <Chip size="sm" color={liveStateColor(lesson.liveState)} variant="soft">
                        {liveStateText(lesson)}
                      </Chip>
                      {lesson.classroom && (
                        <span className="text-xs text-muted">{lesson.classroom}</span>
                      )}
                    </div>
                    <div className="text-sm font-medium text-foreground truncate group-hover:text-primary transition-colors">
                      <HighlightWeek name={lesson.targetName} />
                    </div>
                    <div className="text-xs text-muted flex items-center gap-1 mt-1">
                      <LessonTime lesson={lesson} />
                    </div>
                  </div>
                  <LuMonitorPlay
                    className="text-muted group-hover:text-primary transition-colors mt-1 shrink-0"
                    aria-hidden="true"
                  />
                </button>
              </Card>
            ))}
          </div>
        )}
      </PageSurface>
    </AuthGate>
  );
}
