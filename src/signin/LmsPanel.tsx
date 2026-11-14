/**
 * 畅课签到面板：待签列表 + 交互模态框（数字 / 雷达 / 扫码）。
 */
import { useCallback, useEffect, useState } from "react";
import { Button, Card, Dialog, LoadingIndicator, TextField } from "material/react";
import { LuRefreshCw } from "react-icons/lu";
import * as tauriCore from "@tauri-apps/api/core";
import { bindField, toast } from "../ui";
import type { Building, ResolvedPoint, RollcallItem } from "./types";
import { needAnswer, rollcallMethod } from "./types";
import { RadarForm, type PointSelection } from "./RadarForm";
import { QrSection } from "./QrSection";

const METHOD_LABEL: Record<string, string> = {
  radar: "雷达",
  number: "数字",
  qr: "扫码",
  unknown: "其它",
};

interface LmsPanelProps {
  buildings: Building[];
}

export function LmsPanel({ buildings }: LmsPanelProps) {
  const [rollcalls, setRollcalls] = useState<RollcallItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [active, setActive] = useState<RollcallItem | null>(null);

  const load = useCallback(async (force: boolean) => {
    if (force) setRefreshing(true);
    else setLoading(true);
    try {
      setRollcalls(await tauriCore.invoke<RollcallItem[]>("signin_get_lms_rollcalls", { force }));
    } catch (error) {
      toast.danger("获取待签到列表失败", { description: String(error) });
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    void load(false);
  }, [load]);

  const pending = rollcalls.filter(needAnswer);

  return (
    <section aria-label="畅课待签到列表" className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-4">
        <h2 className="text-lg font-medium">待签到项</h2>
        <Button variant="tonal" disabled={refreshing || loading} onClick={() => void load(true)}>
          <LuRefreshCw className={refreshing ? "animate-spin" : undefined} aria-hidden="true" />
          刷新
        </Button>
      </div>

      {loading ? (
        <div className="flex justify-center py-10">
          <LoadingIndicator />
        </div>
      ) : pending.length === 0 ? (
        <Card variant="outlined" className="p-4 text-sm text-muted">
          当前没有待签到的点名。课程开始后会自动探测并在这里出现。
        </Card>
      ) : (
        pending.map((item) => (
          <Card
            key={item.rollcall_id}
            variant="outlined"
            className="p-4 flex items-center justify-between gap-3 cursor-pointer"
            onClick={() => setActive(item)}
          >
            <div className="flex flex-col gap-1">
              <span className="font-medium">{item.course_title || `课程 ${item.course_id}`}</span>
              <span className="text-xs text-muted">
                点名 #{item.rollcall_id} · {item.status ?? "进行中"}
              </span>
            </div>
            <span className="text-xs font-medium text-primary">
              {METHOD_LABEL[rollcallMethod(item)] ?? METHOD_LABEL.unknown}
            </span>
          </Card>
        ))
      )}

      {active && (
        <RollcallModal
          item={active}
          buildings={buildings}
          onClose={() => setActive(null)}
          onSubmitted={() => {
            setActive(null);
            void load(true);
          }}
        />
      )}
    </section>
  );
}

function RollcallModal({
  item,
  buildings,
  onClose,
  onSubmitted,
}: {
  item: RollcallItem;
  buildings: Building[];
  onClose: () => void;
  onSubmitted: () => void;
}) {
  const method = rollcallMethod(item);
  return (
    <Dialog open headline={item.course_title || "签到"} onClose={onClose}>
      <div className="flex flex-col gap-3">
        {method === "number" && <NumberForm item={item} onSubmitted={onSubmitted} />}
        {method === "radar" && (
          <RadarSection item={item} buildings={buildings} onSubmitted={onSubmitted} />
        )}
        {method === "qr" && <QrSection rollcallId={item.rollcall_id} courseId={item.course_id} />}
        {method === "unknown" && (
          <p className="text-sm text-muted">
            该点名的签到类型（{item.kind ?? "未知"}）暂不支持在应用内完成。
          </p>
        )}
      </div>
      <div slot="actions">
        <Button variant="text" onClick={onClose}>
          关闭
        </Button>
      </div>
    </Dialog>
  );
}

function NumberForm({ item, onSubmitted }: { item: RollcallItem; onSubmitted: () => void }) {
  const [code, setCode] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const submit = async () => {
    setSubmitting(true);
    try {
      await tauriCore.invoke("signin_submit_lms_number", {
        rollcallId: item.rollcall_id,
        code: code.trim(),
      });
      toast.success("数字签到提交成功");
      onSubmitted();
    } catch (error) {
      toast.danger("数字签到失败", { description: String(error) });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <TextField
        label="签到数字码"
        aria-label="输入签到数字码"
        inputMode="numeric"
        maxlength={8}
        {...bindField(setCode)}
      />
      <Button type="submit" variant="filled" disabled={submitting || !code.trim()}>
        {submitting ? "提交中…" : "提交"}
      </Button>
    </form>
  );
}

function RadarSection({
  item,
  buildings,
  onSubmitted,
}: {
  item: RollcallItem;
  buildings: Building[];
  onSubmitted: () => void;
}) {
  const [initial, setInitial] = useState<ResolvedPoint | null>(null);

  useEffect(() => {
    let active = true;
    void tauriCore
      .invoke<ResolvedPoint>("signin_resolve_radar_point", {
        courseId: item.course_id,
        courseTitle: item.course_title,
      })
      .then((point) => {
        if (active) setInitial(point);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [item.course_id, item.course_title]);

  const submit = async (selection: PointSelection) => {
    const args: Record<string, unknown> = {
      rollcallId: item.rollcall_id,
      courseId: item.course_id,
      courseTitle: item.course_title,
    };
    if (selection.kind === "building") {
      args.buildingId = selection.buildingId;
    } else if (selection.kind === "custom") {
      args.customLatitude = selection.latitude;
      args.customLongitude = selection.longitude;
      args.customIsWgs84 = selection.isWgs84;
    }
    try {
      await tauriCore.invoke("signin_submit_lms_radar", args);
      toast.success("雷达签到提交成功");
      onSubmitted();
    } catch (error) {
      toast.danger("雷达签到失败", { description: String(error) });
    }
  };

  return <RadarForm buildings={buildings} initial={initial} onSubmit={submit} />;
}
