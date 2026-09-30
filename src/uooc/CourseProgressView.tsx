import { Chip, ProgressBar, Spinner } from "@heroui/react";
import { attendanceText, type ProgressEntry } from "./types";

export function CourseProgressView({ entry }: { entry?: ProgressEntry }) {
  const data = entry?.data;
  return (
    <div className="grid gap-2" aria-label="服务端课程进度">
      <div className="flex items-center justify-between gap-2 text-sm">
        <span>课程总进度</span>
        <span>{data?.totalPercent == null ? "—" : `${data.totalPercent}%`}</span>
      </div>
      {data?.totalPercent != null && (
        <ProgressBar aria-label="课程总进度" value={data.totalPercent}>
          <ProgressBar.Track>
            <ProgressBar.Fill />
          </ProgressBar.Track>
        </ProgressBar>
      )}
      <div className="grid gap-2 text-sm sm:grid-cols-2">
        {(
          [
            ["视频", data?.video],
            ["测验", data?.test],
            ["作业", data?.homework],
            ["签到", data?.signin],
          ] as const
        ).map(([name, metric]) => (
          <div key={name} className="flex flex-wrap justify-between gap-2">
            <span className="text-muted">{name}</span>
            <span>
              {metric?.percent == null ? "—" : `${metric.percent}%`} · {metric?.count ?? "—"}/
              {metric?.total ?? "—"}
            </span>
          </div>
        ))}
      </div>
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
    </div>
  );
}
