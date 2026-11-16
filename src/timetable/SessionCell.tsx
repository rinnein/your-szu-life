import { checkinMeta, type SessionStatus } from "./merge";
import { courseTone } from "./colors";
import type { SessionCheckin } from "./types";

/** 单元格放不下完整文案时的缩写；完整状态仍在详情对话框里。 */
const CHECKIN_SHORT: Record<SessionCheckin, string | undefined> = {
  pending_lms: "待签·畅课",
  pending_szu: "待签·SZU",
  signed_lms: "已签到",
  signed_szu: "已签到",
  none: undefined,
};

type Props = {
  title: string;
  location?: string;
  teacher?: string;
  timeLabel?: string;
  checkin: SessionCheckin;
  status: SessionStatus;
  /** 占用的作息块数量，决定格子内能放下多少行信息。 */
  span: number;
  onClick: () => void;
};

/**
 * 课表网格里的课程格。
 *
 * 每门课按名字哈希取首页待办同一套 accent 色板：上完的课用 ghost（虚线、
 * 无底色、淡出），当前/下一节加粗课程名。列宽在手机上只有四十多像素，
 * 所以信息按优先级依次塞入——课程名 → 状态 → 教师/地点 → 时间，
 * 超出卡片可显示高度的部分直接截断（省略号），完整信息交给点击后的详情。
 */
export function SessionCell({
  title,
  location,
  teacher,
  timeLabel,
  checkin,
  status,
  span,
  onClick,
}: Props) {
  const tone = courseTone(title);
  const meta = checkinMeta(checkin);
  const short = CHECKIN_SHORT[checkin];
  const focused = status === "current" || status === "next";
  const ghosted = status === "finished";
  const pending = checkin === "pending_lms" || checkin === "pending_szu";
  const badge =
    status === "current"
      ? { text: "上课中", className: "text-primary" }
      : status === "next"
        ? { text: "下一节", className: "text-primary" }
        : short
          ? { text: short, className: meta?.tone === "success" ? "text-success" : "text-warning" }
          : undefined;
  const detail = [span >= 3 ? teacher : undefined, location].filter(Boolean).join(" · ");
  const ring = pending
    ? "ring-1 ring-warning/60"
    : status === "current"
      ? "ring-1 ring-primary/40"
      : "";
  return (
    <button
      type="button"
      title={[title, timeLabel, detail, meta?.text].filter(Boolean).join(" · ")}
      className={`flex min-h-0 w-full flex-1 flex-col gap-0.5 overflow-hidden rounded-md px-1 py-0.5 text-left leading-tight transition-colors ${
        ghosted
          ? `border border-dashed ${tone.bar} bg-transparent text-muted opacity-55`
          : `border-y-0 border-r-0 border-l-[3px] ${tone.bar} ${tone.soft}`
      } ${ring}`}
      onClick={onClick}
    >
      <span
        className={`${span >= 3 ? "line-clamp-4" : span >= 2 ? "line-clamp-3" : "line-clamp-2"} min-w-0 break-words text-[10px] lg:text-xs ${
          focused ? "font-semibold" : "font-medium"
        }`}
      >
        {title}
      </span>
      {badge && (
        <span className={`block break-words text-[9px] ${badge.className}`}>{badge.text}</span>
      )}
      {detail && (
        <span
          className={`${span >= 2 ? "block" : "hidden lg:block"} ${
            span >= 3 ? "line-clamp-3" : "line-clamp-2"
          } break-words text-[10px] text-muted`}
        >
          {detail}
        </span>
      )}
      {span >= 2 && timeLabel && (
        <span className="hidden break-words text-[10px] text-muted tabular-nums lg:block">
          {timeLabel}
        </span>
      )}
    </button>
  );
}
