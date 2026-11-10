//! 更新频道偏好的唯一持久化入口。
//!
//! 频道存在 `settings.sqlite` 的 `update.channel` 里（`"stable"` / `"canary"`），
//! 与 `app_content` 的内存状态解耦：`ContentService` 只接受构造时传入的频道，
//! 切换时的落盘由 command 负责，服务本身保持可单测。

use super::model::UpdateChannel;
use crate::preferences;

pub(crate) fn load() -> UpdateChannel {
    preferences::load::<UpdateChannel>(preferences::UPDATE_CHANNEL)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn save(channel: UpdateChannel) -> Result<UpdateChannel, String> {
    preferences::save(preferences::UPDATE_CHANNEL, &channel)?;
    Ok(channel)
}
