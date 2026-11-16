import { Chip } from "../ui";
import { checkinMeta, type SessionStatus } from "./merge";
import type { SessionCheckin } from "./types";

type Props = {
  title: string;
  periodLabel?: string;
  timeLabel?: string;
  teacher?: string;
  location?: string;
  checkin: SessionCheckin;
  status: SessionStatus;
  note?: string;
  onClick: () => void;
};

const STATUS_CLASS: Record<SessionStatus, string> = {
  finished: "border-separator opacity-50",
  current: "border-primary bg-accent-soft",
  next: "border-primary/60 bg-primary/5",
  future: "border-separator hover:bg-default",
};

export function SessionCard({
  title,
  periodLabel,
  timeLabel,
  teacher,
  location,
  checkin,
  status,
  note,
  onClick,
}: Props) {
  const meta = checkinMeta(checkin);
  const pending = checkin === "pending_lms" || checkin === "pending_szu";
  const ring = pending
    ? "ring-1 ring-warning/60"
    : status === "current"
      ? "ring-1 ring-primary/30"
      : "";
  return (
    <button
      type="button"
      className={`w-full rounded-inner border p-2 text-left transition-colors ${
        STATUS_CLASS[status]
      } ${ring}`}
      onClick={onClick}
    >
      <div className="flex items-start justify-between gap-2">
        <span className="min-w-0 flex-1">
          <span className="block font-medium leading-snug break-words">{title}</span>
          {(periodLabel || timeLabel) && (
            <span className="block text-xs text-muted tabular-nums">
              {[periodLabel, timeLabel].filter(Boolean).join(" · ")}
            </span>
          )}
          {(teacher || location) && (
            <span className="block text-xs text-muted break-words">
              {[teacher, location].filter(Boolean).join(" · ")}
            </span>
          )}
          {note && <span className="block text-xs text-muted">{note}</span>}
        </span>
        <span className="flex shrink-0 flex-col items-end gap-1">
          {status === "current" && (
            <Chip size="sm" color="accent" variant="soft">
              上课中
            </Chip>
          )}
          {status === "next" && (
            <Chip size="sm" color="accent-2" variant="soft">
              下一节
            </Chip>
          )}
          {meta && (
            <Chip size="sm" color={meta.tone} variant="soft">
              {meta.text}
            </Chip>
          )}
        </span>
      </div>
    </button>
  );
}
