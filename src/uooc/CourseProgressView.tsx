import { Chip, ProgressBar, Spinner } from "@heroui/react";
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
    <div className="grid gap-2" aria-label="服务端课程进度">
      <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
        <span>周程进度 · {course.weeks.trim() || "—"}</span>
        <span className="text-muted">结课日期：{endDate}</span>
      </div>
      {schedule ? (
        <ProgressBar
          aria-label="周程进度"
          value={schedule.percent}
          color={schedule.color}
          valueLabel={`${course.weeks}，结课日期：${endDate}`}
        >
          <ProgressBar.Track>
            <ProgressBar.Fill />
          </ProgressBar.Track>
        </ProgressBar>
      ) : (
        <p className="text-xs text-muted">周程进度未知</p>
      )}
      <div className="flex items-center justify-between gap-2 text-sm">
        <span>课程总进度{weighted && "（按权重）"}</span>
        <span>{totalText}</span>
      </div>
      {totalPercent != null && (
        <ProgressBar
          aria-label="课程总进度"
          value={totalPercent}
          valueLabel={
            partial ? `已确认至少 ${formatPercent(displayPercent)}` : formatPercent(totalPercent)
          }
        >
          <ProgressBar.Track>
            <ProgressBar.Fill />
          </ProgressBar.Track>
        </ProgressBar>
      )}
      {weightMessage && <p className="text-xs text-muted">{weightMessage}</p>}
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted">
        <Chip
          size="sm"
          color={
            data?.attendance === "today" || data?.attendance === "complete" ? "success" : "default"
          }
        >
          {attendanceText[data?.attendance ?? "unknown"]}
        </Chip>
        {entry?.refreshing && <Spinner size="sm" aria-label="正在刷新课程进度" />}
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
