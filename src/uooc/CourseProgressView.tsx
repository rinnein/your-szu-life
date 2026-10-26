import { LoadingIndicator, Progress } from "material/react";
import { Chip } from "../ui";
import { attendanceText, type ProgressEntry, type UoocCourse } from "./types";
import { courseSchedule } from "./course-schedule";
import { CourseProgressDetails, formatPercent } from "./CourseProgressDetails";

export function CourseProgressView({
  entry,
  course,
  onRefresh,
}: {
  entry?: ProgressEntry;
  course: UoocCourse;
  onRefresh?: () => void;
}) {
  const data = entry?.data;
  const schedule = courseSchedule(course.weeks);
  const endDate = course.endDate?.trim() || "—";
  const weighted = data?.weightedPercent !== undefined;
  const totalPercent = weighted ? data?.weightedPercent : data?.totalPercent;
  const partial = weighted && data?.weightedPartial;
  const displayPercent =
    partial && totalPercent != null ? Math.floor(totalPercent * 100) / 100 : totalPercent;
  const totalText = `${partial && totalPercent != null ? "≥" : ""}${formatPercent(displayPercent)}`;
  const weightMessage = !weighted
    ? undefined
    : data?.weightTotal == null
      ? "权重信息不完整，暂无法计算加权进度。"
      : data.weightTotal === 0
        ? "此课程未设置考核权重。"
        : Math.abs(data.weightTotal - 100) > 0.000001
          ? `权重合计为 ${formatPercent(data.weightTotal)}，暂无法汇总。`
          : partial
            ? "部分项目的完成率未知，当前仅显示已确认的加权进度。"
            : undefined;
  return (
    <div className="grid gap-4" aria-label="服务端课程进度">
      <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
        <span>周程进度 · {course.weeks.trim() || "—"}</span>
        <span className="text-muted">结课日期：{endDate}</span>
      </div>
      {schedule ? (
        <Progress
          variant="linear"
          aria-label="周程进度"
          value={schedule.percent}
          max={100}
          color={schedule.color}
        />
      ) : (
        <p className="text-xs text-muted">周程进度未知</p>
      )}
      {totalPercent != null && (
        <div className="flex items-center gap-4 rounded-xl border border-separator/50 p-4">
          <div className="relative inline-flex items-center justify-center shrink-0">
            <Progress
              variant="circular"
              aria-label="课程总进度"
              value={Math.max(0, Math.min(100, totalPercent))}
              max={100}
            />
            <span className="absolute text-xs font-semibold">{totalText}</span>
          </div>
          <div className="grid gap-1">
            <span className="text-sm font-semibold">课程总进度{weighted && "（按权重）"}</span>
            {partial && (
              <span className="text-xs text-muted">部分项目进度未知，数值为已确认部分</span>
            )}
          </div>
        </div>
      )}
      {weightMessage && <p className="text-xs text-muted">{weightMessage}</p>}
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted">
        <Chip
          size="sm"
          color={
            data?.attendance === "today" || data?.attendance === "complete"
              ? "success"
              : data?.attendance === "needed"
                ? "warning"
                : "default"
          }
        >
          {attendanceText[data?.attendance ?? "unknown"]}
        </Chip>
        {entry?.refreshing && <LoadingIndicator size={16} aria-label="正在刷新课程进度" />}
        {entry?.error && <span role="status">进度刷新失败：{entry.error}</span>}
        {entry?.updatedAtMs && (
          <span>
            更新于{" "}
            {new Date(entry.updatedAtMs).toLocaleTimeString("zh-CN", {
              hour: "2-digit",
              minute: "2-digit",
              timeZone: "Asia/Shanghai",
            })}
          </span>
        )}
      </div>
      <CourseProgressDetails data={data} title={course.title} onRefresh={onRefresh} />
    </div>
  );
}
