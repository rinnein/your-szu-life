//! 存放在加密凭据文档里的跨模块数据类型。
//!
//! 这些类型既被 [`super::entries`] 的文档结构使用，也被各自的业务模块使用。
//! 放在 core 里是为了让凭据存储不必反向依赖功能 crate；业务模块通过
//! `pub use` 继续沿用原来的路径。

use serde::{Deserialize, Serialize};

/// OJ 账号凭据。文档中拆成「账号」与「会话」两键存放，读取时再合并。
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OjSecret {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub cookies: String,
}

/// 学业认证（WebVPN / ehall）的一组会话 Cookie 快照。
#[derive(Default, Serialize, Deserialize)]
pub struct EnrollmentSecrets {
    pub student: String,
    pub vpn: String,
    pub campus: String,
    pub webvpn: String,
    pub ehall: String,
    #[serde(default)]
    pub academic_context: Option<serde_json::Value>,
    pub campus_token: String,
    pub webvpn_token: String,
}

impl EnrollmentSecrets {
    pub fn validate_cookie_snapshots(&self) -> std::result::Result<(), String> {
        use crate::webview_cookies::{Kind, SessionEnvelope};
        for (raw, kind) in [
            (&self.vpn, Kind::Gateway),
            (&self.campus, Kind::Campus),
            (&self.webvpn, Kind::Webvpn),
            (&self.ehall, Kind::Academic),
        ] {
            if let Some(envelope) = SessionEnvelope::decode(raw)? {
                if envelope.scope.kind != kind {
                    return Err("学业认证 Cookie 范围无效".into());
                }
            }
        }
        Ok(())
    }
}
