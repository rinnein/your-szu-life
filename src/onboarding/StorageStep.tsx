import { Button, Card } from "material/react";
import { Banner } from "../ui";
import { usePending } from "../use-pending";
import {
  DirectoryConfirmDialog,
  DirectoryDefaultGuideDialog,
  DirectoryPickerActions,
  useDirectoryPicker,
} from "../file-manager/DirectoryPicker";
import { useFileManagerState } from "../file-manager/useFileManagerState";
import { useRegisterNextAction } from "../step-next-action";
import type { StepProps } from "./step-types";

/**
 * 初始化向导的强制步骤：确认文件保存目录。
 *
 * 底栏「下一步」在这里等同于点「默认」：尚未确认过目录时先走默认目录的确认
 * 流程（Android 还要经系统选择器授权），确认后自动进入下一步。
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
    label: configured ? "保存并继续" : "下一步",
    // 读取失败时不再永久禁用底栏：给一个重试入口，而不是让用户卡在这一步。
    disabled: picker.busy || loading || advancing,
    busy: picker.busy || advancing,
    run: async () => {
      if (error) {
        await refresh();
        return;
      }
      if (!configured) {
        // 没有主动选过目录时按「默认」处理：与点按钮一样先预览、再确认。
        await picker.requestDefault(state?.platform);
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
      <DirectoryDefaultGuideDialog controller={picker} defaultPath={state?.defaultBase ?? ""} />
    </Card>
  );
}
