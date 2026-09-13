use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum NoticeCode {
    LoadTimeout,
    LoginTimeout,
    FormTimeout,
    NavigationFailed,
    CredentialsUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NetworkFeePortalNotice {
    pub(crate) code: NoticeCode,
    pub(crate) message: &'static str,
}

impl From<NoticeCode> for NetworkFeePortalNotice {
    fn from(code: NoticeCode) -> Self {
        let message = match code {
            NoticeCode::LoadTimeout => {
                "网费官网加载超时，请检查网络后重试。应用未进行充值或套餐变更。"
            }
            NoticeCode::LoginTimeout => {
                "网费登录等待超时，请重新打开官网检查登录状态。应用不会自动重复提交。"
            }
            NoticeCode::FormTimeout => "未能及时确认网费登录表单，请稍后重新打开官网。",
            NoticeCode::NavigationFailed => "无法打开网费官网，请检查网络后重试。",
            NoticeCode::CredentialsUnavailable => {
                "无法读取当前账户的校园卡号和密码，请返回设置或重新认证。"
            }
        };
        Self { code, message }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum PageKind {
    Loading,
    Pending,
    LoginReady,
    Captcha,
    LoginError,
    InvalidForm,
    Portal,
    Other,
    Stale,
    Submitted,
    Manual,
    Toolbar,
    Returning,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PageObservation {
    pub(super) kind: PageKind,
    pub(super) document_token: String,
}

pub(super) fn parse_observation(value: &str) -> Option<PageObservation> {
    if value.len() > 4096 {
        return None;
    }
    let mut value: serde_json::Value = serde_json::from_str(value).ok()?;
    // WebView engines differ in whether the callback's JSON is nested in a string.
    for _ in 0..2 {
        if let serde_json::Value::String(inner) = &value {
            value = serde_json::from_str(inner).ok()?;
        } else {
            break;
        }
    }
    let result: PageObservation = serde_json::from_value(value).ok()?;
    (!result.document_token.is_empty() && result.document_token.len() <= 128).then_some(result)
}
