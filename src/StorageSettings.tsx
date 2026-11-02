import { Card } from "material/react";
import { Banner } from "./ui";
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
  const { state, error, refresh } = useFileManagerState();
  const picker = useDirectoryPicker(() => refresh());

  return (
    <div className="grid gap-4">
      {error && (
        <Banner status="danger">
          <Banner.Title>无法读取保存目录</Banner.Title>
          <Banner.Description>{error}</Banner.Description>
        </Banner>
      )}

      <Card variant="outlined">
        <div slot="headline">统一保存目录</div>
        <div slot="subhead">
          录像等由你直接管理的文件统一保存在这里，各业务模块使用其中的子目录。目录已有其它内容时，
          会统一放进其中的「泥深」子目录。
        </div>
        <div className="p-4">
          <DirectoryPickerActions state={state} controller={picker} />
        </div>
      </Card>

      <Card variant="outlined">
        <div slot="headline">模块子目录</div>
        <div slot="subhead">
          {state?.platform === "android"
            ? "Android 通过系统文件夹选择器授权目录，应用只在你选中的目录里读写。"
            : "文件由系统文件系统直接写入，与系统「文档」目录同级。"}
        </div>
        <div className="p-4 grid gap-3">
          {(state?.modules ?? []).map((module) => (
            <div key={module.id} className="grid gap-0.5">
              <span className="text-sm text-foreground">{module.label}</span>
              <span className="text-xs text-muted break-all">{module.path}</span>
            </div>
          ))}
          {!state && <span className="text-sm text-muted">正在读取模块目录…</span>}
        </div>
      </Card>

      {state?.access.state === "denied" && state.config != null && (
        <Banner status="warning">
          <Banner.Title>需要重新授权目录</Banner.Title>
          <Banner.Description>
            {state.access.detail ?? "系统文件夹授权可能已被撤销。重新选择同一个目录即可恢复保存。"}
          </Banner.Description>
        </Banner>
      )}

      <DirectoryConfirmDialog controller={picker} />
    </div>
  );
}
