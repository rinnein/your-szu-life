import { Button, Card, LoadingIndicator } from "material/react";
import { Banner, SettingList, SettingRow } from "./ui";
import type { Async } from "./async-state";
import { usePending } from "./use-pending";
import type { FileManagerState } from "./file-manager/api";
import {
  DirectoryConfirmDialog,
  DirectoryPickerActions,
  useDirectoryPicker,
} from "./file-manager/DirectoryPicker";
import { useFileManagerState } from "./file-manager/useFileManagerState";

/**
 * 设置 → 储存：管理用户可直接获取/管理的文件统一目录。
 *
 * 选中的目录下按业务模块建子目录（目前只有督导巡课录像 jxxk）；目录为空时
 * 直接使用该目录，已有其它内容时统一放进其中的「泥深」子目录。
 */
export default function StorageSettings() {
  const pending = usePending();
  const { state, error, loading, refresh } = useFileManagerState();
  const picker = useDirectoryPicker(() => refresh());

  // 读取快照只有四种结果：显式区分，失败时给出重试入口，而不是留一行「读取中…」。
  const view: Async<FileManagerState> =
    error != null
      ? { status: "error", message: error, previous: state ?? undefined }
      : state != null
        ? { status: "ready", data: state }
        : loading
          ? { status: "loading" }
          : { status: "idle" };
  const current =
    view.status === "ready" ? view.data : view.status === "error" ? view.previous : undefined;

  return (
    <>
      {view.status === "loading" && (
        <p className="status-line">
          <LoadingIndicator size={16} aria-label="正在读取保存目录" />
          正在读取保存目录…
        </p>
      )}

      {view.status === "error" && (
        <div className="grid justify-items-start gap-3">
          <Banner status="danger">
            <Banner.Title>无法读取保存目录</Banner.Title>
            <Banner.Description>{view.message}</Banner.Description>
          </Banner>
          <Button
            variant="outlined"
            disabled={pending.isPending("reload")}
            onClick={() => void pending.run("reload", () => refresh())}
          >
            重试
          </Button>
        </div>
      )}

      {current && (
        <>
          <Card variant="outlined">
            <div slot="headline">统一保存目录</div>
            <div slot="subhead">
              录像等由你直接管理的文件统一保存在这里，各业务模块使用其中的子目录。目录已有其它内容时，
              会统一放进其中的「泥深」子目录。
            </div>
            <DirectoryPickerActions state={current} controller={picker} />
          </Card>

          <Card variant="outlined">
            <div slot="headline">模块子目录</div>
            <div slot="subhead">
              {current.platform === "android"
                ? "Android 通过系统文件夹选择器授权目录，应用只在你选中的目录里读写。"
                : "文件由系统文件系统直接写入，与系统「文档」目录同级。"}
            </div>
            <SettingList>
              {current.modules.map((module) => (
                <SettingRow
                  key={module.id}
                  title={module.label}
                  description={<span className="break-all">{module.path}</span>}
                />
              ))}
            </SettingList>
          </Card>
        </>
      )}

      <DirectoryConfirmDialog controller={picker} />
    </>
  );
}
