//! 初始化设置（Onboarding）的定义、状态计算与命令入口。
//!
//! 步骤顺序由 [`ONBOARDING_DEFINITION`] 决定；每项的 `revision` 在需求变化时
//! 递增，使旧版本完成/跳过的行自动回到 `pending`。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod commands;

pub const BASELINE_MARKER: &str = "__baseline__";
pub const WELCOME: &str = "welcome";
pub const AUTH: &str = "auth";
pub const SYNC: &str = "sync";
pub const STORAGE: &str = "storage";
pub const DORMITORY: &str = "dormitory";
pub const APPEARANCE: &str = "appearance";
pub const LOCAL_MODEL: &str = "local_model";
pub const AI: &str = "ai";
pub const MCP: &str = "mcp";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OnboardingStatus {
    #[default]
    Pending,
    Configured,
    Skipped,
}

impl OnboardingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Configured => "configured",
            Self::Skipped => "skipped",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "configured" => Some(Self::Configured),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnboardingDefinition {
    pub id: &'static str,
    pub revision: u32,
    pub skippable: bool,
}

pub const ONBOARDING_DEFINITION: [OnboardingDefinition; 9] = [
    OnboardingDefinition {
        id: WELCOME,
        revision: 1,
        skippable: false,
    },
    OnboardingDefinition {
        id: AUTH,
        revision: 1,
        skippable: false,
    },
    OnboardingDefinition {
        id: SYNC,
        revision: 1,
        skippable: true,
    },
    OnboardingDefinition {
        id: STORAGE,
        revision: 1,
        skippable: false,
    },
    OnboardingDefinition {
        id: DORMITORY,
        revision: 1,
        skippable: true,
    },
    OnboardingDefinition {
        id: APPEARANCE,
        revision: 1,
        skippable: false,
    },
    OnboardingDefinition {
        id: LOCAL_MODEL,
        revision: 1,
        skippable: true,
    },
    OnboardingDefinition {
        id: AI,
        revision: 1,
        skippable: true,
    },
    OnboardingDefinition {
        id: MCP,
        revision: 1,
        skippable: false,
    },
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingStepView {
    pub id: String,
    pub status: OnboardingStatus,
    pub skippable: bool,
    pub revision: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingStateView {
    pub steps: Vec<OnboardingStepView>,
    pub needs_setup: bool,
    pub current_step: Option<String>,
}

pub fn definition(id: &str) -> Option<&'static OnboardingDefinition> {
    ONBOARDING_DEFINITION.iter().find(|item| item.id == id)
}

pub fn initialize(_app: &tauri::AppHandle, local_models_ready: bool) -> Result<(), String> {
    crate::migrations::onboarding::adopt_existing_settings(local_models_ready)
}

/// 是否存在完整账户资料。用于 auth 步骤的窄例外：资料已完整时不再强制重登。
pub fn has_complete_profile() -> bool {
    use crate::auth::model::{is_campus_card_number, is_student_id};
    use crate::storage::CredentialStore;

    crate::storage::PlatformCredentialStore
        .load_profile()
        .ok()
        .flatten()
        .is_some_and(|profile| {
            is_student_id(&profile.student_id)
                && is_campus_card_number(&profile.campus_card_number)
                && !profile.password.is_empty()
        })
}

pub fn compute_state(
    definitions: &[OnboardingDefinition],
    rows: &BTreeMap<String, (String, u32)>,
    has_profile: bool,
    demo_active: bool,
) -> OnboardingStateView {
    let satisfied: Vec<bool> = definitions
        .iter()
        .map(|definition| {
            rows.get(definition.id).is_some_and(|(status, revision)| {
                *revision >= definition.revision
                    && OnboardingStatus::parse(status).is_some_and(|status| {
                        status != OnboardingStatus::Pending
                    })
            })
        })
        .collect();
    // 欢迎页只面向全新初始化：其它步骤已有进度（或资料已完整）的老用户不再呈现。
    let has_progress = has_profile
        || definitions
            .iter()
            .zip(&satisfied)
            .any(|(definition, ok)| *ok && definition.id != WELCOME);
    let steps: Vec<OnboardingStepView> = definitions
        .iter()
        .enumerate()
        .map(|(index, definition)| {
            let status = if satisfied[index] {
                rows.get(definition.id)
                    .and_then(|(status, _)| OnboardingStatus::parse(status))
                    .unwrap_or(OnboardingStatus::Configured)
            } else if definition.id == AUTH && has_profile {
                OnboardingStatus::Configured
            } else if definition.id == WELCOME && has_progress {
                OnboardingStatus::Configured
            } else {
                OnboardingStatus::Pending
            };
            OnboardingStepView {
                id: definition.id.to_owned(),
                status,
                skippable: definition.skippable,
                revision: definition.revision,
            }
        })
        .collect();
    // 演示模式只在前端展示 mock 数据，不参与真实进度；此期间向导视为已完成，
    // 退出演示后恢复原状，不动任何已保存的步骤行。
    let needs_setup = !demo_active
        && steps
            .iter()
            .any(|step| step.status == OnboardingStatus::Pending);
    let current_step = if demo_active {
        None
    } else {
        steps
            .iter()
            .find(|step| step.status == OnboardingStatus::Pending)
            .map(|step| step.id.clone())
    };
    OnboardingStateView {
        steps,
        needs_setup,
        current_step,
    }
}

pub fn state_view() -> Result<OnboardingStateView, String> {
    let has_profile = has_complete_profile();
    let rows = crate::preferences::load_onboarding_rows()?;
    let state = compute_state(&ONBOARDING_DEFINITION, &rows, has_profile, crate::demo::is_enabled());
    // 资料已完整时把 auth 的派生结果补写回 DB，避免桌面端登录后重载又把它当未完成。
    if has_profile {
        if let Some(definition) = definition(AUTH) {
            let needs_write = rows
                .get(AUTH)
                .is_none_or(|(_, revision)| *revision < definition.revision);
            if needs_write {
                crate::preferences::save_onboarding_step(
                    AUTH,
                    OnboardingStatus::Configured.as_str(),
                    definition.revision,
                )?;
            }
        }
    }
    Ok(state)
}

/// 一次同步应用成功后，把已经被同步覆盖的步骤直接记为已完成。
///
/// 只处理「数据确实已经存在」的两项：宿舍信息与 AI 配置。其它步骤（文件目录、
/// 外观、本地模型、MCP）都是设备相关的，绝不因为同步成功就跳过。
pub fn adopt_synced_items() {
    let facts = crate::migrations::onboarding::collect_existing_settings_facts(false);
    let rows = crate::preferences::load_onboarding_rows().unwrap_or_default();
    for (id, satisfied) in [
        (DORMITORY, facts.has_electricity_location),
        (AI, facts.has_ai_connection),
    ] {
        if !satisfied {
            continue;
        }
        let Some(definition) = definition(id) else {
            continue;
        };
        let already = rows.get(id).is_some_and(|(status, revision)| {
            *revision >= definition.revision && status != OnboardingStatus::Pending.as_str()
        });
        if already {
            continue;
        }
        if let Err(error) = crate::preferences::save_onboarding_step(
            id,
            OnboardingStatus::Configured.as_str(),
            definition.revision,
        ) {
            log::warn!("同步完成后标记初始化步骤失败（{id}）: {error}");
        }
    }
}

pub fn complete_step(id: &str) -> Result<OnboardingStateView, String> {
    let definition = definition(id).ok_or_else(|| "未知的初始化设置项".to_string())?;
    crate::preferences::save_onboarding_step(
        definition.id,
        OnboardingStatus::Configured.as_str(),
        definition.revision,
    )?;
    state_view()
}

pub fn skip_step(id: &str) -> Result<OnboardingStateView, String> {
    let definition = definition(id).ok_or_else(|| "未知的初始化设置项".to_string())?;
    if !definition.skippable {
        return Err("该设置项不能跳过".into());
    }
    crate::preferences::save_onboarding_step(
        definition.id,
        OnboardingStatus::Skipped.as_str(),
        definition.revision,
    )?;
    state_view()
}

/// 重新开始初始化向导：把当前定义的全部步骤写回 pending。
///
/// 只重置步骤进度，不动任何已保存的设置；保留 `__baseline__`，避免升级时的
/// 沿用逻辑在下一次启动把步骤又自动标记为已完成。
pub fn restart() -> Result<OnboardingStateView, String> {
    for definition in ONBOARDING_DEFINITION {
        crate::preferences::save_onboarding_step(
            definition.id,
            OnboardingStatus::Pending.as_str(),
            0,
        )?;
    }
    state_view()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(entries: &[(&str, &str, u32)]) -> BTreeMap<String, (String, u32)> {
        entries
            .iter()
            .map(|(id, status, revision)| (id.to_string(), (status.to_string(), *revision)))
            .collect()
    }

    #[test]
    fn definition_order_matches_the_required_sequence() {
        let ids: Vec<_> = ONBOARDING_DEFINITION.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![
                WELCOME, AUTH, SYNC, STORAGE, DORMITORY, APPEARANCE, LOCAL_MODEL, AI, MCP
            ]
        );
        assert!(!definition(WELCOME).unwrap().skippable);
        assert!(!definition(AUTH).unwrap().skippable);
        assert!(definition(SYNC).unwrap().skippable);
        assert!(!definition(STORAGE).unwrap().skippable);
        assert!(definition(DORMITORY).unwrap().skippable);
        assert!(!definition(APPEARANCE).unwrap().skippable);
        assert!(definition(LOCAL_MODEL).unwrap().skippable);
        assert!(definition(AI).unwrap().skippable);
        assert!(!definition(MCP).unwrap().skippable);
    }

    #[test]
    fn completed_and_skipped_rows_satisfy_the_current_revision() {
        let state = compute_state(
            &ONBOARDING_DEFINITION,
            &rows(&[
                (AUTH, "configured", 1),
                (SYNC, "skipped", 1),
                (STORAGE, "configured", 1),
                (DORMITORY, "skipped", 1),
                (APPEARANCE, "configured", 1),
                (LOCAL_MODEL, "skipped", 1),
                (AI, "skipped", 1),
                (MCP, "configured", 1),
            ]),
            false,
            false,
        );
        assert!(!state.needs_setup);
        assert_eq!(state.current_step, None);
    }

    #[test]
    fn revision_bump_makes_a_completed_item_pending_again() {
        let mut definitions = ONBOARDING_DEFINITION;
        let ai_index = definitions
            .iter()
            .position(|item| item.id == AI)
            .expect("AI 步骤应存在于定义中");
        definitions[ai_index].revision = 2;
        let state = compute_state(
            &definitions,
            &rows(&[
                (AUTH, "configured", 1),
                (SYNC, "configured", 1),
                (STORAGE, "configured", 1),
                (DORMITORY, "configured", 1),
                (APPEARANCE, "configured", 1),
                (LOCAL_MODEL, "skipped", 1),
                (AI, "configured", 1),
                (MCP, "configured", 1),
            ]),
            false,
            false,
        );
        let ai = state.steps.iter().find(|step| step.id == AI).unwrap();
        assert_eq!(ai.status, OnboardingStatus::Pending);
        assert!(state.needs_setup);
        assert_eq!(state.current_step.as_deref(), Some(AI));
    }

    #[test]
    fn complete_profile_derives_auth_as_configured_without_a_row() {
        let state = compute_state(&ONBOARDING_DEFINITION, &rows(&[]), true, false);
        let auth = state.steps.iter().find(|step| step.id == AUTH).unwrap();
        assert_eq!(auth.status, OnboardingStatus::Configured);
        // 新安装的同步步骤紧跟在认证之后，此时应为待完成。
        assert_eq!(state.current_step.as_deref(), Some(SYNC));
    }

    #[test]
    fn pending_rows_are_not_satisfied() {
        let state = compute_state(
            &ONBOARDING_DEFINITION,
            &rows(&[(AUTH, "pending", 1)]),
            false,
            false,
        );
        let auth = state.steps.iter().find(|step| step.id == AUTH).unwrap();
        assert_eq!(auth.status, OnboardingStatus::Pending);
    }

    #[test]
    fn fresh_install_starts_at_the_welcome_step() {
        let state = compute_state(&ONBOARDING_DEFINITION, &rows(&[]), false, false);
        assert!(state.needs_setup);
        assert_eq!(state.current_step.as_deref(), Some(WELCOME));
        let welcome = state.steps.iter().find(|step| step.id == WELCOME).unwrap();
        assert_eq!(welcome.status, OnboardingStatus::Pending);
    }

    #[test]
    fn existing_progress_skips_the_welcome_step() {
        let state = compute_state(
            &ONBOARDING_DEFINITION,
            &rows(&[(STORAGE, "configured", 1)]),
            false,
            false,
        );
        let welcome = state.steps.iter().find(|step| step.id == WELCOME).unwrap();
        assert_eq!(welcome.status, OnboardingStatus::Configured);
        assert_eq!(state.current_step.as_deref(), Some(AUTH));
    }

    #[test]
    fn demo_mode_defers_the_wizard_without_touching_rows() {
        let rows = rows(&[(STORAGE, "configured", 1)]);
        let state = compute_state(&ONBOARDING_DEFINITION, &rows, false, true);
        assert!(!state.needs_setup);
        assert_eq!(state.current_step, None);
        // 步骤行保持原样，退出演示后恢复真实进度。
        let resumed = compute_state(&ONBOARDING_DEFINITION, &rows, false, false);
        assert!(resumed.needs_setup);
        assert_eq!(resumed.current_step.as_deref(), Some(AUTH));
    }
}
