use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AccountProfileView {
    #[serde(rename = "studentId")]
    pub student_id: String,
    #[serde(rename = "campusCardNumber")]
    pub campus_card_number: String,
    pub name: Option<String>,
    #[serde(rename = "hasPassword")]
    pub has_password: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct UserDataView {
    #[serde(rename = "studentId")]
    pub student_id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AccountProfileInput {
    #[serde(rename = "studentId")]
    pub student_id: String,
    #[serde(rename = "campusCardNumber")]
    pub campus_card_number: String,
    pub name: Option<String>,
    pub password: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct StoredAccountProfile {
    pub(crate) student_id: String,
    pub(crate) campus_card_number: String,
    pub(crate) name: Option<String>,
    pub(crate) password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct PendingAuthCredentials {
    pub(crate) student_id: String,
    pub(crate) password: String,
    pub(crate) name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CapturedCredentials {
    pub(crate) identifier: String,
    pub(crate) password: String,
}

pub(super) fn verified_password_update<'a>(
    saved_student_id: &str,
    saved_campus_card: Option<&str>,
    verified_student_id: &str,
    submitted: Option<&'a CapturedCredentials>,
) -> Option<&'a str> {
    submitted
        .filter(|capture| {
            saved_student_id == verified_student_id
                && (capture.identifier == saved_student_id
                    || Some(capture.identifier.as_str()) == saved_campus_card)
                && !capture.password.is_empty()
        })
        .map(|capture| capture.password.as_str())
}

/// Exists only while the official login page is being prepared. This is
/// serialized directly into an `eval` script for the trusted official login
/// document, never into a command result, event, log, or local app state.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct LoginFormPrefill {
    pub(crate) identifier: String,
    pub(crate) password: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AuthState {
    Checking,
    Unauthenticated,
    Authenticating,
    AwaitingProfile {
        name: Option<String>,
        student_id: Option<String>,
    },
    Authenticated {
        name: Option<String>,
        student_id: String,
    },
    CredentialCaptureFailed,
    SessionCaptureFailed,
    CredentialStorageFailed,
    AccountMismatch,
    SessionExpired,
    WebViewUpdateRequired,
}

impl Default for AuthState {
    fn default() -> Self {
        Self::Checking
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AuthAttempt {
    None,
    InitialSetup,
    ReauthenticateExisting { student_id: String },
}

impl AuthAttempt {
    pub(super) fn allows_qr_login(&self) -> bool {
        matches!(self, Self::ReauthenticateExisting { .. })
    }
}
pub(super) fn profile_view(profile: &StoredAccountProfile) -> AccountProfileView {
    AccountProfileView {
        student_id: profile.student_id.clone(),
        campus_card_number: profile.campus_card_number.clone(),
        name: profile.name.clone(),
        has_password: !profile.password.is_empty(),
    }
}

pub(super) fn user_data_view(
    profile: Option<&StoredAccountProfile>,
    pending: Option<&PendingAuthCredentials>,
) -> Option<UserDataView> {
    profile
        .map(|profile| UserDataView {
            student_id: profile.student_id.clone(),
            name: profile.name.clone(),
        })
        .or_else(|| {
            pending.map(|pending| UserDataView {
                student_id: pending.student_id.clone(),
                name: pending.name.clone(),
            })
        })
}

pub(super) fn login_form_prefill(
    profile: Option<&StoredAccountProfile>,
    pending: Option<&PendingAuthCredentials>,
) -> Option<LoginFormPrefill> {
    profile
        .map(|profile| LoginFormPrefill {
            identifier: profile.student_id.clone(),
            password: profile.password.clone(),
        })
        .or_else(|| {
            pending.map(|pending| LoginFormPrefill {
                identifier: pending.student_id.clone(),
                password: pending.password.clone(),
            })
        })
}

pub(crate) fn is_student_id(value: &str) -> bool {
    value.len() == 10 && value.chars().all(|character| character.is_ascii_digit())
}

pub(crate) fn is_campus_card_number(value: &str) -> bool {
    value.len() == 6 && value.chars().all(|character| character.is_ascii_digit())
}

impl AccountProfileInput {
    pub(super) fn validate(&self, current_password: Option<&str>) -> Result<(), String> {
        if !is_student_id(&self.student_id) {
            return Err("学号必须是 10 位数字".into());
        }
        if !is_campus_card_number(&self.campus_card_number) {
            return Err("校园卡号必须是 6 位数字".into());
        }
        if self
            .password
            .as_deref()
            .unwrap_or(current_password.unwrap_or(""))
            .is_empty()
        {
            return Err("密码不能为空".into());
        }
        Ok(())
    }
}
