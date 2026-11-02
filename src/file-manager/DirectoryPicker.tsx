import { useCallback, useState } from "react";
import { createPortal } from "react-dom";
import { Button, Dialog } from "material/react";
import { LuFolderOpen, LuRotateCcw } from "react-icons/lu";
import { Banner, toast } from "../ui";
import {
  pickFileManagerDirectory,
  releaseFileManagerPick,
  saveFileManagerDirectory,
  useDefaultFileManagerDirectory,
  type DirectoryPreview,
  type FileManagerState,
} from "./api";

export type DirectoryPickerController = {
  busy: boolean;
  preview: DirectoryPreview | null;
  pick: () => Promise<void>;
  useDefault: () => Promise<void>;
  confirm: () => Promise<void>;
  cancel: () => void;
};

/**
 * 目录选择流程：打开系统选择器 → 预览 → 二次确认落库。
 *
 * 取消确认框时释放刚取得的 Android SAF 持久授权，避免授权条目泄漏。
 * `onSaved` 在落库成功后调用（设置页刷新、向导推进下一步）。
 */
export function useDirectoryPicker(
  onSaved?: (state: FileManagerState) => void | Promise<void>,
): DirectoryPickerController {
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<DirectoryPreview | null>(null);

  const pick = useCallback(async () => {
    setBusy(true);
    try {
      const next = await pickFileManagerDirectory();
      if (next) setPreview(next);
    } catch (cause) {
      toast.danger("无法打开文件夹选择器", { description: String(cause) });
    } finally {
      setBusy(false);
    }
  }, []);

  const useDefault = useCallback(async () => {
    setBusy(true);
    try {
      setPreview(await useDefaultFileManagerDirectory());
    } catch (cause) {
      toast.danger("无法使用默认目录", { description: String(cause) });
    } finally {
      setBusy(false);
    }
  }, []);

  const confirm = useCallback(async () => {
    if (!preview) return;
    setBusy(true);
    try {
      const next = await saveFileManagerDirectory(preview.selection);
      setPreview(null);
      toast.success("保存目录已更新", { description: next.base });
      await onSaved?.(next);
    } catch (cause) {
      toast.danger("保存目录失败", { description: String(cause) });
    } finally {
      setBusy(false);
    }
  }, [preview, onSaved]);

  const cancel = useCallback(() => {
    const released = preview?.selection;
    setPreview(null);
    if (released) void releaseFileManagerPick(released).catch(() => undefined);
  }, [preview]);

  return { busy, preview, pick, useDefault, confirm, cancel };
}

/** 当前目录 + 选择/恢复默认按钮。 */
export function DirectoryPickerActions({
  state,
  controller,
  disabled = false,
}: {
  state: FileManagerState | null;
  controller: DirectoryPickerController;
  disabled?: boolean;
}) {
  const busy = controller.busy || disabled;
  const accessBroken = state != null && state.config != null && state.access.state === "denied";
  return (
    <div className="grid gap-2">
      {accessBroken && (
        <Banner status="danger">
          <Banner.Title>保存目录不可用</Banner.Title>
          <Banner.Description>
            {state?.access.detail ?? "请重新选择保存目录，以重新获得系统授权。"}
          </Banner.Description>
        </Banner>
      )}
      <div className="text-xs text-muted">当前保存目录</div>
      <div className="text-sm text-foreground break-all">{state?.base ?? "读取中…"}</div>
      <div className="flex items-center gap-2 mt-1">
        <Button variant="tonal" disabled={busy} onClick={() => void controller.pick()}>
          <LuFolderOpen aria-hidden="true" />
          选择目录
        </Button>
        {state?.canUseDefault && (
          <Button
            variant="text"
            disabled={busy}
            aria-label="使用默认目录"
            onClick={() => void controller.useDefault()}
          >
            <LuRotateCcw aria-hidden="true" />
          </Button>
        )}
      </div>
      {state?.usingDefault && (
        <p className="text-xs text-muted">当前使用平台默认目录，尚未自定义选择。</p>
      )}
    </div>
  );
}

/** 选完目录后的二次确认。 */
export function DirectoryConfirmDialog({ controller }: { controller: DirectoryPickerController }) {
  const preview = controller.preview;
  if (!preview) return null;
  return createPortal(
    <Dialog
      open
      onClose={controller.cancel}
      headline="确认保存目录"
      size="small"
      actions={
        <>
          <Button variant="text" disabled={controller.busy} onClick={controller.cancel}>
            取消
          </Button>
          <Button
            variant="filled"
            disabled={controller.busy}
            onClick={() => void controller.confirm()}
          >
            确认
          </Button>
        </>
      }
    >
      <div className="grid gap-3 text-sm">
        <div>
          <div className="text-xs text-muted">所选目录</div>
          <div className="break-all text-foreground">{preview.selected}</div>
        </div>
        <div>
          <div className="text-xs text-muted">
            {preview.selectedWasEmpty
              ? "该目录为空，文件直接保存在"
              : "该目录已有其它内容，文件将统一保存在"}
          </div>
          <div className="break-all text-foreground">{preview.base}</div>
        </div>
        {preview.notice && <p className="text-xs text-muted">{preview.notice}</p>}
        <div className="grid gap-1">
          <div className="text-xs text-muted">各模块子目录</div>
          {preview.modules.map((module) => (
            <div key={module.id} className="text-xs text-foreground break-all">
              {module.label}：{module.path}
            </div>
          ))}
        </div>
      </div>
    </Dialog>,
    document.body,
  );
}
