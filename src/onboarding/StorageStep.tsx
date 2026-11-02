import { useState } from "react";
import { Card } from "material/react";
import { Banner, toast } from "../ui";
import {
  DirectoryConfirmDialog,
  DirectoryPickerActions,
  useDirectoryPicker,
} from "../file-manager/DirectoryPicker";
import { useFileManagerState } from "../file-manager/useFileManagerState";
import { useRegisterNextAction } from "../step-next-action";
import type { StepProps } from "./step-types";

/**
 * 初始化向导的强制步骤：确认文件保存目录。
 *
 * Android 上必须由用户在系统文件夹选择器里授权目录，桌面端可以直接选目录或
 * 用默认目录（按钮旁的 reset 图标）。
 */
export function StorageStep({ onComplete, registerNext }: StepProps) {
  const { state, error, loading, refresh } = useFileManagerState();
  const picker = useDirectoryPicker(async () => {
    await refresh();
    await onComplete();
  });
  const [advancing, setAdvancing] = useState(false);

  const configured = state?.config != null;

  useRegisterNextAction(registerNext, {
    label: configured ? "保存并继续" : "选择保存位置",
    disabled: picker.busy || loading || advancing || Boolean(error),
    busy: picker.busy || advancing,
    run: async () => {
      if (!configured) {
        await picker.pick();
        return;
      }
      setAdvancing(true);
      try {
        await onComplete();
      } catch (cause) {
        toast.danger("保存初始化设置进度失败", { description: String(cause) });
      } finally {
        setAdvancing(false);
      }
    },
  });

  return (
    <Card variant="outlined">
      <div slot="headline">文件保存位置</div>
      <div slot="subhead">
        录像等由你直接管理的文件会统一保存在这里。请选择一个文件夹：目录为空时直接使用它，
        已有其它内容时会放在其中的「泥深」子目录。
        {state?.platform === "android"
          ? " Android 会弹出系统文件夹选择器，授权只对这个目录生效。"
          : ""}
      </div>
      <div className="p-4 grid gap-3">
        {error && (
          <Banner status="danger">
            <Banner.Title>无法读取保存目录</Banner.Title>
            <Banner.Description>{error}</Banner.Description>
          </Banner>
        )}
        <DirectoryPickerActions state={state} controller={picker} />
      </div>
      <DirectoryConfirmDialog controller={picker} />
    </Card>
  );
}
