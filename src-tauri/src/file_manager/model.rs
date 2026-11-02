//! 文件目录模块对外暴露的数据结构。
//!
//! 所有类型都会跨 IPC 与 MCP 边界，因此同时实现 `Serialize` / `Deserialize` /
//! `JsonSchema`；字段描述即工具文档。

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 在统一文件目录下按业务模块划分的子目录。
///
/// 新增模块时在这里加一个变体，并同步 `ALL`、`id` 与 `label`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ManagedModule {
    /// 督导巡课录像。
    Jxxk,
}

impl ManagedModule {
    pub(crate) const ALL: [Self; 1] = [Self::Jxxk];

    /// 落盘子目录名。
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Jxxk => "jxxk",
        }
    }

    /// 设置页展示名。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Jxxk => "督导巡课录像",
        }
    }
}

/// 目录选择来源。
///
/// 桌面端是原生选择器返回的绝对路径；Android 是 SAF（存储访问框架）文档树
/// URI —— Android 11 起只有 SAF 授权，没有按真实路径写入的权限。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum DirectorySelection {
    /// 桌面端绝对路径。
    Filesystem {
        /// 绝对路径。
        path: String,
    },
    /// Android SAF 文档树 URI。
    AndroidTree {
        /// `content://com.android.externalstorage.documents/tree/...`
        uri: String,
    },
    /// 使用平台默认目录（仅桌面提供）。
    Default,
}

impl DirectorySelection {
    pub(crate) fn android_tree(&self) -> Option<&str> {
        match self {
            Self::AndroidTree { uri } => Some(uri.as_str()),
            _ => None,
        }
    }
}

/// 已经确认并落库的目录配置。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirectoryConfig {
    /// 选择来源。
    pub selection: DirectorySelection,
    /// Android 树内相对父目录：空串表示直接使用所选目录，`泥深` 表示追加品牌目录。
    /// 桌面端不使用（父目录就是 `displayRoot` 本身）。
    #[serde(default)]
    pub relative_base: String,
    /// 生效父目录的展示路径。Android 为尽力还原的真实路径，桌面为绝对路径。
    pub display_root: String,
    /// 用户选中的目录（展示用；Android 与 `displayRoot` 可能不同）。
    pub selected: String,
    /// 是否来自「恢复默认目录」。
    #[serde(default)]
    pub via_default: bool,
    /// 选择时间（Unix 毫秒）。
    pub chosen_at: i64,
}

/// 选择器返回、尚未落库的目录预览。
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirectoryPreview {
    /// 确认时原样回传的选项。
    pub selection: DirectorySelection,
    /// 用户选中的目录。
    pub selected: String,
    /// 所选目录是否为空——决定是否追加「泥深」子目录。
    pub selected_was_empty: bool,
    /// 实际生效的父目录。
    pub base: String,
    /// 各模块最终目录。
    pub modules: Vec<ModuleDirectory>,
    /// 需要额外提示用户的说明（例如 Android 上目录非空）。
    pub notice: Option<String>,
}

/// 单个模块的最终目录。
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModuleDirectory {
    /// 模块 id，如 `jxxk`。
    pub id: String,
    /// 模块展示名。
    pub label: String,
    /// 最终目录（Android 为展示路径）。
    pub path: String,
}

/// 目录访问状态。
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StorageAccess {
    /// `granted` | `denied` | `notApplicable`
    pub state: String,
    /// 面向用户的补充说明。
    pub detail: Option<String>,
}

/// 文件目录模块的完整状态快照。
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileManagerState {
    /// `android` | `windows` | `linux` | `macos`
    pub platform: String,
    /// 平台默认父目录（含「泥深」）。
    pub default_base: String,
    /// 当前生效的配置；为 `None` 表示尚未选择过目录。
    pub config: Option<DirectoryConfig>,
    /// 当前生效的父目录。
    pub base: String,
    /// 是否正在使用平台默认目录。
    pub using_default: bool,
    /// 目录访问状态。
    pub access: StorageAccess,
    /// 各模块最终目录。
    pub modules: Vec<ModuleDirectory>,
    /// 桌面端是否支持「恢复默认目录」（Android 上必须由用户在系统选择器中授权）。
    pub can_use_default: bool,
}
