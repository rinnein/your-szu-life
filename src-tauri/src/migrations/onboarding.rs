//! 老版本应用已有设置的确认逻辑。
//!
//! 首次运行新版本时，把“确实已经配置好”的项目直接记为 `configured`，只让用户
//! 补做缺失的步骤。执行结果通过保留行 `__baseline__` 标记，之后不再重复。

use crate::onboarding::{OnboardingStatus, BASELINE_MARKER};
use crate::preferences::{APPEARANCE, MCP_ENABLED};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ExistingSettingsFacts {
    pub has_profile: bool,
    pub has_electricity_location: bool,
    pub has_appearance: bool,
    pub has_mcp_choice: bool,
    pub has_ai_connection: bool,
    pub local_models_ready: bool,
}

pub(crate) fn collect_existing_settings_facts() -> ExistingSettingsFacts {
    use crate::storage::CredentialStore;

    let has_profile = crate::onboarding::has_complete_profile();
    let has_electricity_location = crate::storage::PlatformCredentialStore
        .load_electricity_settings()
        .ok()
        .flatten()
        .is_some();
    let has_appearance =
        crate::preferences::load::<crate::appearance::Appearance>(APPEARANCE)
            .ok()
            .flatten()
            .is_some();
    let has_mcp_choice = crate::preferences::load::<bool>(MCP_ENABLED)
        .ok()
        .flatten()
        .is_some();
    let has_ai_connection = crate::storage::PlatformCredentialStore
        .load_uooc_ai()
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .is_some_and(|config| {
            let main = &config["main"];
            main.get("baseUrl")
                .and_then(|value| value.as_str())
                .is_some_and(|value| !value.is_empty())
                && main
                    .get("key")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| !value.is_empty())
        });
    let local_models_ready = crate::ocr::status().is_ok_and(|status| status.status == "ready")
        && crate::embedding_model::status().is_ok_and(|status| status.status == "ready");

    ExistingSettingsFacts {
        has_profile,
        has_electricity_location,
        has_appearance,
        has_mcp_choice,
        has_ai_connection,
        local_models_ready,
    }
}

pub(crate) fn adoption_plan(facts: &ExistingSettingsFacts) -> Vec<&'static str> {
    let mut plan = Vec::new();
    if facts.has_profile {
        plan.push(crate::onboarding::AUTH);
    }
    if facts.has_electricity_location {
        plan.push(crate::onboarding::DORMITORY);
    }
    if facts.has_appearance {
        plan.push(crate::onboarding::APPEARANCE);
    }
    if facts.local_models_ready {
        plan.push(crate::onboarding::LOCAL_MODEL);
    }
    if facts.has_ai_connection {
        plan.push(crate::onboarding::AI);
    }
    if facts.has_mcp_choice {
        plan.push(crate::onboarding::MCP);
    }
    plan
}

pub(crate) fn apply_adoption_plan(
    facts: &ExistingSettingsFacts,
    rows: &BTreeMap<String, (String, u32)>,
    mut write: impl FnMut(&'static str, u32) -> Result<(), String>,
) -> Result<Vec<&'static str>, String> {
    let mut written = Vec::new();
    for id in adoption_plan(facts) {
        if rows.contains_key(id) {
            continue;
        }
        let definition = crate::onboarding::definition(id)
            .ok_or_else(|| format!("未知的初始化设置项: {id}"))?;
        write(id, definition.revision)?;
        written.push(id);
    }
    Ok(written)
}

pub(crate) fn adopt_existing_settings() -> Result<(), String> {
    let rows = crate::preferences::load_onboarding_rows()?;
    if rows.contains_key(BASELINE_MARKER) {
        return Ok(());
    }
    let facts = collect_existing_settings_facts();
    apply_adoption_plan(&facts, &rows, |id, revision| {
        crate::preferences::save_onboarding_step(
            id,
            OnboardingStatus::Configured.as_str(),
            revision,
        )
    })?;
    crate::preferences::save_onboarding_step(
        BASELINE_MARKER,
        OnboardingStatus::Configured.as_str(),
        1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_false() -> ExistingSettingsFacts {
        ExistingSettingsFacts::default()
    }

    #[test]
    fn empty_install_adopts_nothing() {
        assert!(adoption_plan(&all_false()).is_empty());
    }

    #[test]
    fn plan_maps_facts_to_definition_order() {
        let facts = ExistingSettingsFacts {
            has_profile: true,
            has_electricity_location: true,
            has_appearance: true,
            local_models_ready: true,
            has_ai_connection: true,
            has_mcp_choice: true,
        };
        assert_eq!(
            adoption_plan(&facts),
            vec![crate::onboarding::AUTH, crate::onboarding::DORMITORY, crate::onboarding::APPEARANCE, crate::onboarding::LOCAL_MODEL, crate::onboarding::AI, crate::onboarding::MCP]
        );
    }

    #[test]
    fn existing_rows_are_not_overwritten() {
        let facts = ExistingSettingsFacts {
            has_profile: true,
            has_mcp_choice: true,
            ..Default::default()
        };
        let rows = BTreeMap::from([(
            crate::onboarding::AUTH.to_string(),
            ("skipped".to_string(), 1),
        )]);
        let mut written = Vec::new();
        let result = apply_adoption_plan(&facts, &rows, |id, revision| {
            written.push((id, revision));
            Ok(())
        })
        .unwrap();
        assert_eq!(result, vec![crate::onboarding::MCP]);
        assert_eq!(written, vec![(crate::onboarding::MCP, 1)]);
    }
}
