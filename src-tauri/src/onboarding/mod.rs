//! 初始化设置（Onboarding）的定义、状态计算与命令入口。
//!
//! 步骤顺序由 [`ONBOARDING_DEFINITION`] 决定；每项的 `revision` 在需求变化时
//! 递增，使旧版本完成/跳过的行自动回到 `pending`。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) mod commands;
pub(crate) mod mcp;

pub(crate) const BASELINE_MARKER: &str = "__baseline__";
pub(crate) const AUTH: &str = "auth";
pub(crate) const DORMITORY: &str = "dormitory";
pub(crate) const APPEARANCE: &str = "appearance";
pub(crate) const LOCAL_MODEL: &str = "local_model";
pub(crate) const AI: &str = "ai";
pub(crate) const MCP: &str = "mcp";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum OnboardingStatus {
    #[default]
    Pending,
    Configured,
    Skipped,
}

impl OnboardingStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Configured => "configured",
            Self::Skipped => "skipped",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "configured" => Some(Self::Configured),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OnboardingDefinition {
    pub id: &'static str,
    pub revision: u32,
    pub skippable: bool,
}

pub(crate) const ONBOARDING_DEFINITION: [OnboardingDefinition; 6] = [
    OnboardingDefinition {
        id: AUTH,
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
pub(crate) struct OnboardingStepView {
    pub id: String,
    pub status: OnboardingStatus,
    pub skippable: bool,
    pub revision: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OnboardingStateView {
    pub steps: Vec<OnboardingStepView>,
    pub needs_setup: bool,
    pub current_step: Option<String>,
}

pub(crate) fn definition(id: &str) -> Option<&'static OnboardingDefinition> {
    ONBOARDING_DEFINITION.iter().find(|item| item.id == id)
}

pub(crate) fn initialize(_app: &tauri::AppHandle) -> Result<(), String> {
    crate::migrations::onboarding::adopt_existing_settings()
}

/// 是否存在完整账户资料。用于 auth 步骤的窄例外：资料已完整时不再强制重登。
pub(crate) fn has_complete_profile() -> bool {
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

pub(crate) fn compute_state(
    definitions: &[OnboardingDefinition],
    rows: &BTreeMap<String, (String, u32)>,
    has_profile: bool,
) -> OnboardingStateView {
    let steps: Vec<OnboardingStepView> = definitions
        .iter()
        .map(|definition| {
            let stored = rows.get(definition.id);
            let stored_status = stored.and_then(|(status, _)| OnboardingStatus::parse(status));
            let satisfied = stored.is_some_and(|(status, revision)| {
                *revision >= definition.revision
                    && OnboardingStatus::parse(status).is_some_and(|status| {
                        status != OnboardingStatus::Pending
                    })
            });
            let status = if satisfied {
                stored_status.unwrap_or(OnboardingStatus::Configured)
            } else if definition.id == AUTH && has_profile {
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
    let needs_setup = steps
        .iter()
        .any(|step| step.status == OnboardingStatus::Pending);
    let current_step = steps
        .iter()
        .find(|step| step.status == OnboardingStatus::Pending)
        .map(|step| step.id.clone());
    OnboardingStateView {
        steps,
        needs_setup,
        current_step,
    }
}

pub(crate) fn state_view() -> Result<OnboardingStateView, String> {
    let has_profile = has_complete_profile();
    let rows = crate::preferences::load_onboarding_rows()?;
    let state = compute_state(&ONBOARDING_DEFINITION, &rows, has_profile);
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

pub(crate) fn complete_step(id: &str) -> Result<OnboardingStateView, String> {
    let definition = definition(id).ok_or_else(|| "未知的初始化设置项".to_string())?;
    crate::preferences::save_onboarding_step(
        definition.id,
        OnboardingStatus::Configured.as_str(),
        definition.revision,
    )?;
    state_view()
}

pub(crate) fn skip_step(id: &str) -> Result<OnboardingStateView, String> {
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
            vec![AUTH, DORMITORY, APPEARANCE, LOCAL_MODEL, AI, MCP]
        );
        assert!(!definition(AUTH).unwrap().skippable);
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
                (DORMITORY, "skipped", 1),
                (APPEARANCE, "configured", 1),
                (LOCAL_MODEL, "skipped", 1),
                (AI, "skipped", 1),
                (MCP, "configured", 1),
            ]),
            false,
        );
        assert!(!state.needs_setup);
        assert_eq!(state.current_step, None);
    }

    #[test]
    fn revision_bump_makes_a_completed_item_pending_again() {
        let mut definitions = ONBOARDING_DEFINITION;
        definitions[4].revision = 2;
        let state = compute_state(
            &definitions,
            &rows(&[
                (AUTH, "configured", 1),
                (DORMITORY, "configured", 1),
                (APPEARANCE, "configured", 1),
                (LOCAL_MODEL, "skipped", 1),
                (AI, "configured", 1),
                (MCP, "configured", 1),
            ]),
            false,
        );
        let ai = state.steps.iter().find(|step| step.id == AI).unwrap();
        assert_eq!(ai.status, OnboardingStatus::Pending);
        assert!(state.needs_setup);
        assert_eq!(state.current_step.as_deref(), Some(AI));
    }

    #[test]
    fn complete_profile_derives_auth_as_configured_without_a_row() {
        let state = compute_state(&ONBOARDING_DEFINITION, &rows(&[]), true);
        let auth = state.steps.iter().find(|step| step.id == AUTH).unwrap();
        assert_eq!(auth.status, OnboardingStatus::Configured);
        assert_eq!(state.current_step.as_deref(), Some(DORMITORY));
    }

    #[test]
    fn pending_rows_are_not_satisfied() {
        let state = compute_state(
            &ONBOARDING_DEFINITION,
            &rows(&[(AUTH, "pending", 1)]),
            false,
        );
        let auth = state.steps.iter().find(|step| step.id == AUTH).unwrap();
        assert_eq!(auth.status, OnboardingStatus::Pending);
    }
}
