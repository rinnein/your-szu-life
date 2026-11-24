import { Button, Dialog } from "material/react";
import { Chip } from "../ui";
import { checkinMeta, isPendingCheckin, isSignedCheckin } from "./merge";
import type { SessionCheckin } from "./types";

type Props = {
  open: boolean;
  title: string;
  periodLabel?: string;
  timeLabel?: string;
  teacher?: string;
  location?: string;
  weeks?: string;
  checkin: SessionCheckin;
  onClose: () => void;
  onSignin?: () => void;
};

function Row({ label, value }: { label: string; value?: string }) {
  if (!value) return null;
  return (
    <div className="flex gap-3 text-sm">
      <span className="w-14 shrink-0 text-muted">{label}</span>
      <span className="min-w-0 break-words">{value}</span>
    </div>
  );
}

export function SessionDetailDialog({
  open,
  title,
  periodLabel,
  timeLabel,
  teacher,
  location,
  weeks,
  checkin,
  onClose,
  onSignin,
}: Props) {
  const meta = checkinMeta(checkin);
  return (
    <Dialog open={open} headline={title} onClose={onClose}>
      <div className="grid gap-2">
        <Row label="时间" value={[periodLabel, timeLabel].filter(Boolean).join(" · ")} />
        <Row label="教师" value={teacher} />
        <Row label="地点" value={location} />
        <Row label="周次" value={weeks} />
        {meta && (
          <div className="flex items-center gap-2 text-sm">
            <span className="w-14 shrink-0 text-muted">签到</span>
            <Chip size="sm" color={meta.tone} variant="soft">
              {meta.text}
            </Chip>
          </div>
        )}
      </div>
      <div slot="actions" className="flex items-center justify-end gap-2">
        {isPendingCheckin(checkin) && onSignin && (
          <Button
            variant="filled"
            onClick={() => {
              onClose();
              onSignin();
            }}
          >
            去签到
          </Button>
        )}
        {isSignedCheckin(checkin) && (
          <Chip size="sm" color="success" variant="soft">
            已签到
          </Chip>
        )}
      </div>
    </Dialog>
  );
}
