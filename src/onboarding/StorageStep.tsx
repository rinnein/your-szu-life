import { Button, Card } from "material/react";
import { Banner } from "../ui";
import { usePending } from "../use-pending";
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
  const pending = usePending();
  const advancing = pending.isPending("advance");

  const configured = state?.config != null;

  useRegisterNextAction(registerNext, {
    label: configured ? "保存并继续" : "选择保存位置",
    // 读取失败时不再永久禁用底栏：给一个重试入口，而不是让用户卡在这一步。
    disabled: picker.busy || loading || advancing,
    busy: picker.busy || advancing,
    run: async () => {
      if (error) {
        await refresh();
        return;
      }
      if (!configured) {
        await picker.pick();
        return;
      }
      // 失败的提示由向导统一给出，这里不重复 toast。
      await pending.run("advance", async () => {
        await onComplete();
      });
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
      <div className="grid gap-3">
        {error && (
          <Banner status="danger">
            <Banner.Title>无法读取保存目录</Banner.Title>
            <Banner.Description>{error}</Banner.Description>
          </Banner>
        )}
        {error && (
          <div>
            <Button variant="outlined" disabled={loading} onClick={() => void refresh()}>
              重新读取
            </Button>
          </div>
        )}
        <DirectoryPickerActions state={state} controller={picker} />
      </div>
      <DirectoryConfirmDialog controller={picker} />
    </Card>
  );
}
