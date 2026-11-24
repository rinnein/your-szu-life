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
  /** Android 上点「默认」不能直接落库，先提示用户去系统选择器授权默认目录。 */
  guide: boolean;
  pick: () => Promise<void>;
  /** 「默认」的统一入口：Android 先弹引导，桌面端直接预览并等待确认。 */
  requestDefault: (platform: string | undefined) => Promise<void>;
  confirm: () => Promise<void>;
  cancel: () => void;
  dismissGuide: () => void;
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
  const [guide, setGuide] = useState(false);

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

  // Android 的标准目录只能经 SAF 授权写入（应用不申请存储权限），因此这里
  // 先说明、再由系统选择器完成授权；选择器的初始位置本身就是默认目录。
  const requestDefault = useCallback(
    async (platform: string | undefined) => {
      if (platform === "android") {
        setGuide(true);
        return;
      }
      await useDefault();
    },
    [useDefault],
  );

  const dismissGuide = useCallback(() => setGuide(false), []);

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

  return {
    busy,
    preview,
    guide,
    pick,
    requestDefault,
    confirm,
    cancel,
    dismissGuide,
  };
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
  // Android 没有「直接使用默认目录」这条路径，但入口保留：它先引导用户去
  // 系统选择器里授权默认目录，桌面端则直接预览默认目录并等待确认。
  const canDefault = state != null && (state.canUseDefault || state.platform === "android");
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
        {canDefault && (
          <Button
            variant="text"
            disabled={busy}
            aria-label="使用默认目录"
            onClick={() => void controller.requestDefault(state?.platform)}
          >
            <LuRotateCcw aria-hidden="true" />
            默认
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

/**
 * Android 上「默认」的额外一步。
 *
 * Android 标准目录不允许应用按真实路径写入，也不申请任何存储权限，只能通过
 * SAF 由用户授权一次。系统选择器的初始位置已经是默认目录，所以这里只需要
 * 说明授权流程，再由用户在选择器里确认。
 */
export function DirectoryDefaultGuideDialog({
  controller,
  defaultPath,
}: {
  controller: DirectoryPickerController;
  defaultPath: string;
}) {
  if (!controller.guide) return null;
  return createPortal(
    <Dialog
      open
      onClose={controller.dismissGuide}
      headline="使用默认目录"
      size="small"
      actions={
        <>
          <Button variant="text" disabled={controller.busy} onClick={controller.dismissGuide}>
            取消
          </Button>
          <Button
            variant="filled"
            disabled={controller.busy}
            onClick={() => {
              controller.dismissGuide();
              void controller.pick();
            }}
          >
            去选择目录
          </Button>
        </>
      }
    >
      <div className="grid gap-3 text-sm">
        <div>
          <div className="text-xs text-muted">默认目录</div>
          <div className="break-all text-foreground">{defaultPath}</div>
        </div>
        <p className="text-xs text-muted">
          Android
          不允许应用直接写入标准目录，需要你在系统文件夹选择器里授权一次；应用不会申请任何存储权限，只在你确认的目录内读写。
        </p>
        <div className="grid gap-1">
          <div className="text-xs text-muted">在系统选择器中</div>
          <div className="text-xs text-foreground">1. 打开「下载 / Download」。</div>
          <div className="text-xs text-foreground">2. 选中或新建「泥深」文件夹。</div>
          <div className="text-xs text-foreground">3. 点击「使用此文件夹」完成授权。</div>
        </div>
      </div>
    </Dialog>,
    document.body,
  );
}
