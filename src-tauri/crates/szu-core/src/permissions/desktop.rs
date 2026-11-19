//! 桌面端没有系统运行时权限：状态统一为 `unsupported`，申请入口直接报错。

use super::{PermissionId, PermissionStateView, PermissionStatus};

pub(super) fn states() -> Vec<PermissionStateView> {
    PermissionId::ALL
        .into_iter()
        .map(|id| PermissionStateView {
            id,
            status: PermissionStatus::Unsupported,
            can_request: false,
            note: None,
        })
        .collect()
}
