/**
 * 畅课点名对话框：按签到方式切换数字 / 雷达 / 扫码表单。
 *
 * 主操作固定在动作区（取消 + 提交），提交过程按 usePending("submit")
 * 登记并在进行中禁止关闭；地点选择是单一 PointSelection，提交参数由
 * selectionArgs 统一翻译。
 */
import { useState } from "react";
import * as tauriCore from "@tauri-apps/api/core";
import { Button, Dialog, LoadingIndicator, TextField } from "material/react";
import { bindField, toast } from "../ui";
import { errorMessage, useAsyncLoad } from "../async-state";
import { usePending } from "../use-pending";
import { PointPicker } from "./PointPicker";
import { selectionArgs, type PointSelection } from "./point-selection";
import { QrSection, type QrActions } from "./QrSection";
import type { QrState } from "./qr-state";
import type { Building, ResolvedPoint, RollcallItem } from "./types";
import { rollcallMethod } from "./types";

interface LmsRollcallDialogProps {
  item: RollcallItem;
  buildings: Building[];
  buildingsFailed?: boolean;
  qrState: QrState;
  qrActions: QrActions;
  onClose: () => void;
  onSubmitted: () => void;
}

export function LmsRollcallDialog({
  item,
  buildings,
  buildingsFailed,
  qrState,
  qrActions,
  onClose,
  onSubmitted,
}: LmsRollcallDialogProps) {
  const method = rollcallMethod(item);
  const pending = usePending();
  const submitting = pending.isPending("submit");
  const [code, setCode] = useState("");
  const [selection, setSelection] = useState<PointSelection>({ kind: "default" });

  // 默认点解析（上次成功 → 课表 → 兜底）；只用于展示，失败仍可提交。
  const resolved = useAsyncLoad<ResolvedPoint | null>(async () => {
    if (method !== "radar") return null;
    return tauriCore.invoke<ResolvedPoint>("signin_resolve_radar_point", {
      courseId: item.course_id,
      courseTitle: item.course_title,
    });
  });
  const resolvedPending = resolved.state.status === "idle" || resolved.state.status === "loading";

  const finishSubmit = (
    outcome: { ok: true } | { ok: false; skipped?: true; error?: unknown },
    successMessage: string,
    failureTitle: string,
  ) => {
    if (outcome.ok) {
      toast.success(successMessage);
      onSubmitted();
      return;
    }
    if (!outcome.skipped) {
      toast.danger(failureTitle, { description: errorMessage(outcome.error) });
    }
  };

  const submitNumber = async () => {
    const trimmed = code.trim();
    if (!trimmed) return;
    const outcome = await pending.run("submit", () =>
      tauriCore.invoke("signin_submit_lms_number", { rollcallId: item.rollcall_id, code: trimmed }),
    );
    finishSubmit(outcome, "数字签到提交成功", "数字签到失败");
  };

  const submitRadar = async () => {
    const outcome = await pending.run("submit", () =>
      tauriCore.invoke("signin_submit_lms_radar", {
        rollcallId: item.rollcall_id,
        courseId: item.course_id,
        courseTitle: item.course_title,
        ...selectionArgs(selection),
      }),
    );
    finishSubmit(outcome, "雷达签到提交成功", "雷达签到失败");
  };

  const canSubmit = method === "number" ? code.trim() !== "" : !resolvedPending;

  return (
    <Dialog
      open
      headline={item.course_title || "签到"}
      size={method === "number" ? "small" : undefined}
      onClose={() => {
        if (!submitting) onClose();
      }}
      actions={
        method === "qr" || method === "unknown" ? (
          <Button variant="text" onClick={onClose}>
            关闭
          </Button>
        ) : (
          <>
            <Button variant="text" disabled={submitting} onClick={onClose}>
              取消
            </Button>
            <Button
              variant="filled"
              disabled={!canSubmit || submitting}
              onClick={() => void (method === "number" ? submitNumber() : submitRadar())}
            >
              {submitting && <LoadingIndicator size={16} />}
              提交
            </Button>
          </>
        )
      }
    >
      <div className="flex flex-col gap-3">
        {method === "number" && (
          <TextField
            label="签到数字码"
            aria-label="输入签到数字码"
            inputMode="numeric"
            maxlength={8}
            value={code}
            onKeyDown={(event: { key?: string; preventDefault?: () => void }) => {
              if (event.key === "Enter") {
                event.preventDefault?.();
                void submitNumber();
              }
            }}
            {...bindField(setCode)}
          />
        )}
        {method === "radar" && (
          <PointPicker
            buildings={buildings}
            buildingsFailed={buildingsFailed}
            resolved={resolved.state}
            value={selection}
            onChange={setSelection}
          />
        )}
        {method === "qr" && (
          <QrSection
            rollcallId={item.rollcall_id}
            courseId={item.course_id}
            state={qrState}
            actions={qrActions}
          />
        )}
        {method === "unknown" && (
          <p className="text-sm text-muted">
            该点名的签到类型（{item.type ?? "未知"}）暂不支持在应用内完成。
          </p>
        )}
      </div>
    </Dialog>
  );
}
