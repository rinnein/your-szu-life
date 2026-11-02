import { invoke } from "@tauri-apps/api/core";

/** 目录选择来源：桌面为绝对路径，Android 为 SAF 文档树 URI。 */
export type DirectorySelection =
  | { kind: "filesystem"; path: string }
  | { kind: "androidTree"; uri: string }
  | { kind: "default" };

export type DirectoryConfig = {
  selection: DirectorySelection;
  /** Android 树内相对父目录：空串或 `泥深`。 */
  relativeBase: string;
  /** 生效父目录的展示路径。 */
  displayRoot: string;
  /** 用户选中的目录（展示用）。 */
  selected: string;
  viaDefault: boolean;
  chosenAt: number;
};

export type ModuleDirectory = {
  id: string;
  label: string;
  path: string;
};

export type StorageAccessState = "granted" | "denied" | "notApplicable";

export type StorageAccess = {
  state: StorageAccessState;
  detail: string | null;
};

export type FileManagerState = {
  platform: string;
  defaultBase: string;
  config: DirectoryConfig | null;
  base: string;
  usingDefault: boolean;
  access: StorageAccess;
  modules: ModuleDirectory[];
  /** 桌面端可恢复默认目录；Android 必须由系统选择器授权。 */
  canUseDefault: boolean;
};

/** 选择器返回、尚未落库的目录预览。 */
export type DirectoryPreview = {
  selection: DirectorySelection;
  selected: string;
  selectedWasEmpty: boolean;
  base: string;
  modules: ModuleDirectory[];
  notice: string | null;
};

/** 保存目录变化事件（后端落库后广播）。 */
export const FILE_MANAGER_CHANGED_EVENT = "file-manager-changed";

export function getFileManagerState(): Promise<FileManagerState> {
  return invoke<FileManagerState>("get_file_manager_state");
}

/** 用户取消原生选择器时返回 null。 */
export function pickFileManagerDirectory(): Promise<DirectoryPreview | null> {
  return invoke<DirectoryPreview | null>("pick_file_manager_directory");
}

export function useDefaultFileManagerDirectory(): Promise<DirectoryPreview> {
  return invoke<DirectoryPreview>("use_default_file_manager_directory");
}

export function saveFileManagerDirectory(selection: DirectorySelection): Promise<FileManagerState> {
  return invoke<FileManagerState>("save_file_manager_directory", { selection });
}

export function releaseFileManagerPick(selection: DirectorySelection): Promise<void> {
  return invoke<void>("release_file_manager_pick", { selection });
}
