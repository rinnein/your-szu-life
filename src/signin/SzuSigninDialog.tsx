/**
 * SZU 签到对话框：地点选择 → 照片（拍照/选择）→ 动作区提交。
 *
 * 照片在 Rust 侧压缩归一为 JPEG 并按课程记忆；无本次照片且无上次照片时
 * 提交不可用（后端要求二者其一），而不是让提交撞服务端错误。
 * Android 用带 capture 的文件选择唤起系统相机/相册。
 */
import { useRef, useState } from "react";
import * as tauriCore from "@tauri-apps/api/core";
import { Button, Dialog, LoadingIndicator } from "material/react";
import { LuCamera, LuImageUp } from "react-icons/lu";
import { toast } from "../ui";
import { errorMessage, useAsyncLoad } from "../async-state";
import { usePending } from "../use-pending";
import { PointPicker } from "./PointPicker";
import { selectionArgs, type PointSelection } from "./point-selection";
import { useAndroid } from "../use-android";
import type { Building, LastPhotoView, ResolvedPoint, TimetableSession } from "./types";

export function SzuSigninDialog({
  session,
  buildings,
  buildingsFailed,
  onClose,
}: {
  session: TimetableSession;
  buildings: Building[];
  buildingsFailed?: boolean;
  onClose: () => void;
}) {
  const [selection, setSelection] = useState<PointSelection>({ kind: "default" });
  const [photo, setPhoto] = useState<string | null>(null); // data URI，base64 在提交时切出
  const pending = usePending();
  const submitting = pending.isPending("submit");
  const savingPhoto = pending.isPending("savePhoto");
  const android = useAndroid() === "android";

  const resolved = useAsyncLoad<ResolvedPoint>(() =>
    tauriCore.invoke<ResolvedPoint>("signin_resolve_radar_point", {
      courseName: session.course_name,
      courseTitle: session.course_name,
    }),
  );
  const lastPhoto = useAsyncLoad<LastPhotoView>(() =>
    tauriCore.invoke<LastPhotoView>("signin_get_last_photo", { courseName: session.course_name }),
  );

  const lastPhotoReady = lastPhoto.state.status === "ready" ? lastPhoto.state.data : null;
  const hasRememberedPhoto = lastPhotoReady?.hasPhoto === true;
  const resolvedPending = resolved.state.status === "idle" || resolved.state.status === "loading";
  const canSubmit = (photo !== null || hasRememberedPhoto) && !resolvedPending;

  const preview = photo ?? lastPhotoReady?.thumbnail ?? null;
  const note = photo
    ? "将使用本次选择的照片提交（提交时压缩为 JPEG）；可先点「保存照片」供之后复用。"
    : hasRememberedPhoto
      ? "将使用该课程上一次保存的签到照片提交，自动签到也会复用它；也可以重新拍照或选择。"
      : "SZU 签到需要照片，请先拍摄或选择一张；也可只保存照片，供自动签到使用。";

  const captureRef = useRef<HTMLInputElement | null>(null);
  const uploadRef = useRef<HTMLInputElement | null>(null);

  const handlePhotoPicked = (files: FileList | null) => {
    const file = files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const dataUri = String(reader.result ?? "");
      if (dataUri.startsWith("data:")) setPhoto(dataUri);
    };
    reader.readAsDataURL(file);
  };

  const submit = async () => {
    const outcome = await pending.run("submit", async () => {
      const args: Record<string, unknown> = {
        courseName: session.course_name,
        ...selectionArgs(selection),
      };
      if (photo) args.photoB64 = photo.slice(photo.indexOf(",") + 1);
      await tauriCore.invoke("signin_submit_szu", args);
    });
    if (outcome.ok) {
      toast.success("SZU 签到提交成功");
      onClose();
    } else if (!outcome.skipped) {
      toast.danger("SZU 签到失败", { description: errorMessage(outcome.error) });
    }
  };

  const savePhoto = async () => {
    if (!photo) return;
    const outcome = await pending.run("savePhoto", () =>
      tauriCore.invoke<LastPhotoView>("signin_save_szu_photo", {
        courseName: session.course_name,
        photoB64: photo.slice(photo.indexOf(",") + 1),
      }),
    );
    if (outcome.ok) {
      lastPhoto.apply(outcome.value);
      toast.success("已保存该课程的签到照片", {
        description: "之后的手动或自动签到会复用这张照片，无需签到成功。",
      });
    } else if (!outcome.skipped) {
      toast.danger("保存签到照片失败", { description: errorMessage(outcome.error) });
    }
  };

  return (
    <Dialog
      open
      headline={session.course_name}
      onClose={() => {
        if (!submitting) onClose();
      }}
      actions={
        <>
          <Button variant="text" disabled={submitting} onClick={onClose}>
            取消
          </Button>
          <Button
            variant="filled"
            disabled={!canSubmit || submitting}
            onClick={() => void submit()}
          >
            {submitting && <LoadingIndicator size={16} />}
            提交
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <PointPicker
          buildings={buildings}
          buildingsFailed={buildingsFailed}
          resolved={resolved.state}
          value={selection}
          onChange={setSelection}
        />

        <div className="flex flex-col gap-2">
          <div className="flex flex-wrap gap-2">
            <input
              ref={captureRef}
              type="file"
              accept="image/*"
              capture={android ? "environment" : undefined}
              aria-label="拍摄签到照片"
              className="hidden"
              data-testid="szu-photo-input"
              onChange={(event) => handlePhotoPicked(event.target.files)}
            />
            <Button variant="tonal" onClick={() => captureRef.current?.click()}>
              <LuCamera aria-hidden="true" />
              拍照
            </Button>
            <input
              ref={uploadRef}
              type="file"
              accept="image/*"
              aria-label="选择签到照片"
              className="hidden"
              data-testid="szu-photo-file-input"
              onChange={(event) => handlePhotoPicked(event.target.files)}
            />
            <Button variant="tonal" onClick={() => uploadRef.current?.click()}>
              <LuImageUp aria-hidden="true" />
              选择照片
            </Button>
            {photo && (
              <Button
                variant="outlined"
                disabled={savingPhoto || submitting}
                onClick={() => void savePhoto()}
              >
                {savingPhoto && <LoadingIndicator size={16} />}
                保存照片
              </Button>
            )}
          </div>
          {preview && (
            <img
              src={preview}
              alt="签到照片预览"
              className="max-h-40 w-fit rounded-inner object-cover"
            />
          )}
          <p className="text-xs text-muted">{note}</p>
        </div>
      </div>
    </Dialog>
  );
}
