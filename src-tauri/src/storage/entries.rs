//! 凭据文档的结构定义与键映射。
//!
//! 该模块是全应用唯一的凭据明文结构来源：本地加密文件 `vault.bin` 与
//! 「设置 → 设置迁移」导出的加密包使用完全相同的 [`VaultDocument`]，
//! 因此两条路径解密后的明文逐字节相同。

use crate::{
    auth::{
        model::{PendingAuthCredentials, StoredAccountProfile},
        session::AuthSessionSecret,
    },
    campus_network::model::{DormCustomCredentials, DormNetworkSettings, StaffNetworkSettings},
    campus_payments::model::{ElectricityLocationSettings, ElectricityMonitorRecord},
    netproxy::ProxySettings,
    oj::model::Secret as OjSecret,
    storage::CredentialStore,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) const FORMAT: u8 = 1;
pub(crate) const MAX_DOCUMENT: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SettingKind {
    Fixed,
    Runtime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SettingEntry {
    pub kind: SettingKind,
    pub value: Value,
}

/// 凭据文档。序列化结果即密钥环加密文件与设置导出包共同的明文格式。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VaultDocument {
    pub version: u8,
    pub entries: BTreeMap<String, SettingEntry>,
}

impl Default for VaultDocument {
    fn default() -> Self {
        Self {
            version: FORMAT,
            entries: BTreeMap::new(),
        }
    }
}

impl VaultDocument {
    pub(crate) fn get<T: for<'a> Deserialize<'a>>(&self, key: &str) -> Result<Option<T>, String> {
        self.entries
            .get(key)
            .map(|entry| serde_json::from_value(entry.value.clone()).map_err(|e| e.to_string()))
            .transpose()
    }

    pub(crate) fn set<T: Serialize>(&mut self, key: &str, value: &T) -> Result<(), String> {
        let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
        self.entries.insert(
            key.to_owned(),
            SettingEntry {
                kind: kind_for(key),
                value,
            },
        );
        Ok(())
    }

    pub(crate) fn remove(&mut self, key: &str) {
        self.entries.remove(key);
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.version != FORMAT {
            return Err("不支持的凭据文档版本".into());
        }
        for (key, entry) in &self.entries {
            if entry.kind != kind_for(key) {
                return Err("凭据条目分类与键不匹配".into());
            }
            validate_value(key, entry.value.clone())?;
        }
        Ok(())
    }
}

pub(crate) fn kind_for(key: &str) -> SettingKind {
    if key.starts_with("runtime.") {
        SettingKind::Runtime
    } else {
        SettingKind::Fixed
    }
}

pub(crate) fn validate_value(key: &str, value: Value) -> Result<(), String> {
    match key {
        PROFILE => {
            serde_json::from_value::<StoredAccountProfile>(value)
                .map_err(|_| "账户设置格式无效")?;
        }
        PENDING_AUTH => {
            serde_json::from_value::<PendingAuthCredentials>(value)
                .map_err(|_| "待确认账户格式无效")?;
        }
        AUTH_SESSION => {
            serde_json::from_value::<AuthSessionSecret>(value).map_err(|_| "登录会话格式无效")?;
        }
        UOOC_AI | UOOC_SESSION | ACADEMIC_SESSIONS => {
            serde_json::from_value::<String>(value).map_err(|_| "设置格式无效")?;
        }
        OJ_ACCOUNTS => {
            serde_json::from_value::<BTreeMap<String, OjAccount>>(value)
                .map_err(|_| "OJ 账号设置格式无效")?;
        }
        OJ_SESSIONS => {
            serde_json::from_value::<BTreeMap<String, String>>(value)
                .map_err(|_| "OJ 会话格式无效")?;
        }
        DORM_SETTINGS => {
            serde_json::from_value::<DormNetworkSettings>(value)
                .map_err(|_| "校园网设置格式无效")?;
        }
        DORM_CREDENTIALS | STAFF_CREDENTIALS => {
            serde_json::from_value::<DormCustomCredentials>(value)
                .map_err(|_| "校园网凭据格式无效")?;
        }
        STAFF_SETTINGS => {
            serde_json::from_value::<StaffNetworkSettings>(value)
                .map_err(|_| "校园网设置格式无效")?;
        }
        ELECTRICITY_LOCATION => {
            serde_json::from_value::<ElectricityLocationSettings>(value)
                .map_err(|_| "电费设置格式无效")?;
        }
        ELECTRICITY_MONITOR => {
            serde_json::from_value::<ElectricityMonitorRecord>(value)
                .map_err(|_| "电费监控记录格式无效")?;
        }
        PROXY_SETTINGS => {
            serde_json::from_value::<ProxySettings>(value)
                .map_err(|_| "网络代理设置格式无效")?
                .validate()
                .map_err(|_| "网络代理设置取值无效")?;
        }
        _ => return Err("设置键无效".into()),
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OjAccount {
    username: String,
    password: String,
    url: String,
}

pub(crate) const PROFILE: &str = "fixed.account.profile";
pub(crate) const PENDING_AUTH: &str = "runtime.account.pending";
pub(crate) const AUTH_SESSION: &str = "runtime.auth.session";
pub(crate) const UOOC_AI: &str = "fixed.uooc.ai";
pub(crate) const UOOC_SESSION: &str = "runtime.uooc.session";
pub(crate) const OJ_ACCOUNTS: &str = "fixed.oj.accounts";
pub(crate) const OJ_SESSIONS: &str = "runtime.oj.sessions";
pub(crate) const ACADEMIC_SESSIONS: &str = "runtime.academic.sessions";
pub(crate) const DORM_SETTINGS: &str = "fixed.network.dorm.settings";
pub(crate) const DORM_CREDENTIALS: &str = "fixed.network.dorm.credentials";
pub(crate) const STAFF_SETTINGS: &str = "fixed.network.staff.settings";
pub(crate) const STAFF_CREDENTIALS: &str = "fixed.network.staff.credentials";
pub(crate) const ELECTRICITY_LOCATION: &str = "fixed.electricity.location";
pub(crate) const ELECTRICITY_MONITOR: &str = "runtime.electricity.monitor";
/// 全局网络代理。属于用户配置（`fixed.`），随设置迁移导出。
pub(crate) const PROXY_SETTINGS: &str = "fixed.network.proxy";

/// 设置导出与本地凭据文档共享的完整键集合。
pub(crate) const ALL_KEYS: [&str; 15] = [
    PROFILE,
    PENDING_AUTH,
    AUTH_SESSION,
    UOOC_AI,
    UOOC_SESSION,
    OJ_ACCOUNTS,
    OJ_SESSIONS,
    ACADEMIC_SESSIONS,
    DORM_SETTINGS,
    DORM_CREDENTIALS,
    STAFF_SETTINGS,
    STAFF_CREDENTIALS,
    ELECTRICITY_LOCATION,
    ELECTRICITY_MONITOR,
    PROXY_SETTINGS,
];

/// 旧版钥匙环条目名与文档键的对应关系，供迁移使用。
pub(crate) const LEGACY_KEYS: [(&str, &str); 13] = [
    ("profile", PROFILE),
    ("pending-auth", PENDING_AUTH),
    ("auth-session", AUTH_SESSION),
    ("uooc-ai", UOOC_AI),
    ("uooc-session", UOOC_SESSION),
    ("oj-accounts", OJ_ACCOUNTS),
    ("academic-sessions", ACADEMIC_SESSIONS),
    ("dorm-network-settings", DORM_SETTINGS),
    ("dorm-network-credentials", DORM_CREDENTIALS),
    ("staff-network-settings", STAFF_SETTINGS),
    ("staff-network-credentials", STAFF_CREDENTIALS),
    ("electricity-settings", ELECTRICITY_LOCATION),
    ("electricity-monitor", ELECTRICITY_MONITOR),
];

/// 把一次 `CredentialStore` 写入落到文档；OJ 账号拆成账号与会话两键。
pub(crate) fn set_raw(
    document: &mut VaultDocument,
    key: crate::storage::SecretKey,
    raw: &str,
) -> Result<(), String> {
    use crate::storage::SecretKey;
    match key {
        SecretKey::OjAccounts => {
            let secrets: BTreeMap<String, OjSecret> =
                serde_json::from_str(raw).map_err(|e| e.to_string())?;
            insert_oj_secrets(document, secrets)
        }
        _ => {
            let name = key.document_key().ok_or("未知的凭据条目")?;
            let value: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
            document.set(name, &value)
        }
    }
}

/// 写入完整的 OJ 账号 JSON 文本（非二次编码）。供 `save_oj_accounts` 使用。
pub(crate) fn set_oj_raw(document: &mut VaultDocument, raw: &str) -> Result<(), String> {
    let secrets: BTreeMap<String, OjSecret> =
        serde_json::from_str(raw).map_err(|e| e.to_string())?;
    insert_oj_secrets(document, secrets)
}

/// 读回完整 OJ 账号（账号 + cookie 会话），供 `load_oj_accounts` 使用。
pub(crate) fn load_oj_accounts(
    store: &crate::storage::vault::Vault,
) -> Result<Option<String>, String> {
    let accounts: Option<BTreeMap<String, OjAccount>> = store.load(OJ_ACCOUNTS)?;
    let sessions: Option<BTreeMap<String, String>> = store.load(OJ_SESSIONS)?;
    let Some(accounts) = accounts else {
        return Ok(None);
    };
    let sessions = sessions.unwrap_or_default();
    let secrets: BTreeMap<String, OjSecret> = accounts
        .into_iter()
        .map(|(id, account)| {
            let cookies = sessions.get(&id).cloned().unwrap_or_default();
            (
                id,
                OjSecret {
                    username: account.username,
                    password: account.password,
                    url: account.url,
                    cookies,
                },
            )
        })
        .collect();
    serde_json::to_string(&secrets)
        .map(Some)
        .map_err(|e| e.to_string())
}

fn insert_oj_secrets(
    document: &mut VaultDocument,
    secrets: BTreeMap<String, OjSecret>,
) -> Result<(), String> {
    let accounts: BTreeMap<String, OjAccount> = secrets
        .iter()
        .map(|(id, secret)| {
            (
                id.clone(),
                OjAccount {
                    username: secret.username.clone(),
                    password: secret.password.clone(),
                    url: secret.url.clone(),
                },
            )
        })
        .collect();
    let sessions: BTreeMap<String, String> = secrets
        .iter()
        .filter(|(_, secret)| !secret.cookies.is_empty())
        .map(|(id, secret)| (id.clone(), secret.cookies.clone()))
        .collect();
    document.set(OJ_ACCOUNTS, &accounts)?;
    document.set(OJ_SESSIONS, &sessions)?;
    Ok(())
}

/// 把旧版钥匙环里的原始 JSON 文本写入文档。
pub(crate) fn insert_legacy(
    document: &mut VaultDocument,
    legacy_name: &str,
    raw: &str,
) -> Result<(), String> {
    use crate::storage::SecretKey;
    let key = SecretKey::from_legacy_name(legacy_name).ok_or("未知的旧版凭据条目")?;
    set_raw(document, key, raw)
}

/// 从 `CredentialStore` 读取单个导出键。
pub(crate) fn read_entry(store: &dyn CredentialStore, key: &str) -> Result<Option<Value>, String> {
    macro_rules! value {
        ($expr:expr) => {
            match $expr? {
                Some(value) => Some(serde_json::to_value(value).map_err(|_| "设置序列化失败")?),
                None => None,
            }
        };
    }
    Ok(match key {
        PROFILE => value!(store.load_profile()),
        PENDING_AUTH => value!(store.load_pending()),
        AUTH_SESSION => value!(store.load_session()),
        UOOC_AI => value!(store.load_uooc_ai()),
        UOOC_SESSION => value!(store.load_uooc_session()),
        ACADEMIC_SESSIONS => value!(store.load_academic_sessions()),
        DORM_SETTINGS => value!(store.load_dorm_settings()),
        DORM_CREDENTIALS => value!(store.load_dorm_credentials()),
        STAFF_SETTINGS => value!(store.load_staff_settings()),
        STAFF_CREDENTIALS => value!(store.load_staff_credentials()),
        ELECTRICITY_LOCATION => value!(store.load_electricity_settings()),
        ELECTRICITY_MONITOR => value!(store.load_electricity_monitor()),
        PROXY_SETTINGS => value!(store.load_proxy_settings()),
        OJ_ACCOUNTS | OJ_SESSIONS => {
            let raw = store.load_oj_accounts()?;
            match raw {
                Some(raw) => {
                    let secrets: BTreeMap<String, OjSecret> =
                        serde_json::from_str(&raw).map_err(|_| "OJ 账号数据损坏")?;
                    if key == OJ_ACCOUNTS {
                        let accounts: BTreeMap<String, OjAccount> = secrets
                            .into_iter()
                            .map(|(id, secret)| {
                                (
                                    id,
                                    OjAccount {
                                        username: secret.username,
                                        password: secret.password,
                                        url: secret.url,
                                    },
                                )
                            })
                            .collect();
                        Some(serde_json::to_value(accounts).map_err(|_| "设置序列化失败")?)
                    } else {
                        let sessions: BTreeMap<String, String> = secrets
                            .into_iter()
                            .filter(|(_, secret)| !secret.cookies.is_empty())
                            .map(|(id, secret)| (id, secret.cookies))
                            .collect();
                        Some(serde_json::to_value(sessions).map_err(|_| "设置序列化失败")?)
                    }
                }
                None => None,
            }
        }
        _ => return Err("设置键无效".into()),
    })
}

/// 把单个导出键写回 `CredentialStore`。
pub(crate) fn write_entry(
    store: &dyn CredentialStore,
    key: &str,
    value: Value,
) -> Result<(), String> {
    macro_rules! save {
        ($ty:ty, $method:ident) => {{
            let value: $ty = serde_json::from_value(value).map_err(|_| "设置值格式无效")?;
            store.$method(&value)
        }};
    }
    match key {
        PROFILE => save!(StoredAccountProfile, save_profile),
        PENDING_AUTH => save!(PendingAuthCredentials, save_pending),
        AUTH_SESSION => save!(AuthSessionSecret, save_session),
        UOOC_AI => save!(String, save_uooc_ai),
        UOOC_SESSION => save!(String, save_uooc_session),
        ACADEMIC_SESSIONS => save!(String, save_academic_sessions),
        DORM_SETTINGS => save!(DormNetworkSettings, save_dorm_settings),
        DORM_CREDENTIALS => save!(DormCustomCredentials, save_dorm_credentials),
        STAFF_SETTINGS => save!(StaffNetworkSettings, save_staff_settings),
        STAFF_CREDENTIALS => save!(DormCustomCredentials, save_staff_credentials),
        ELECTRICITY_LOCATION => save!(ElectricityLocationSettings, save_electricity_settings),
        ELECTRICITY_MONITOR => save!(ElectricityMonitorRecord, save_electricity_monitor),
        PROXY_SETTINGS => save!(ProxySettings, save_proxy_settings),
        OJ_ACCOUNTS | OJ_SESSIONS => write_oj_entry(store, key, value),
        _ => Err("设置键无效".into()),
    }
}

fn write_oj_entry(store: &dyn CredentialStore, key: &str, value: Value) -> Result<(), String> {
    let mut secrets: BTreeMap<String, OjSecret> = store
        .load_oj_accounts()?
        .map(|raw| serde_json::from_str(&raw).map_err(|_| "OJ 账号数据损坏"))
        .transpose()?
        .unwrap_or_default();
    if key == OJ_ACCOUNTS {
        let accounts: BTreeMap<String, OjAccount> =
            serde_json::from_value(value).map_err(|_| "OJ 账号设置格式无效")?;
        for (id, account) in accounts {
            let cookies = secrets
                .get(&id)
                .map(|secret| secret.cookies.clone())
                .unwrap_or_default();
            secrets.insert(
                id,
                OjSecret {
                    username: account.username,
                    password: account.password,
                    url: account.url,
                    cookies,
                },
            );
        }
    } else {
        let sessions: BTreeMap<String, String> =
            serde_json::from_value(value).map_err(|_| "OJ 会话格式无效")?;
        for (id, cookies) in sessions {
            secrets.entry(id).or_default().cookies = cookies;
        }
    }
    store.save_oj_accounts(&serde_json::to_string(&secrets).map_err(|_| "OJ 账号序列化失败")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_classified() {
        for key in ALL_KEYS {
            let expected = if key.starts_with("runtime.") {
                SettingKind::Runtime
            } else {
                SettingKind::Fixed
            };
            assert_eq!(kind_for(key), expected, "{key}");
        }
        let unique: std::collections::BTreeSet<_> = ALL_KEYS.into_iter().collect();
        assert_eq!(unique.len(), ALL_KEYS.len());
        for (_, key) in LEGACY_KEYS {
            assert!(ALL_KEYS.contains(&key), "{key} 未注册");
        }
    }

    #[test]
    fn legacy_oj_accounts_split_into_accounts_and_sessions() {
        let mut document = VaultDocument::default();
        let raw = r#"{"szu-1":{"username":"alice","password":"secret","url":"http://example/","cookies":"c"}}"#;
        insert_legacy(&mut document, "oj-accounts", raw).unwrap();
        let accounts: BTreeMap<String, Value> = document.get(OJ_ACCOUNTS).unwrap().unwrap();
        assert_eq!(accounts["szu-1"]["username"], "alice");
        assert!(accounts["szu-1"].get("cookies").is_none());
        let sessions: BTreeMap<String, String> = document.get(OJ_SESSIONS).unwrap().unwrap();
        assert_eq!(sessions["szu-1"], "c");
    }

    #[test]
    fn oj_accounts_roundtrip_preserves_cookies() {
        // 回归：拆分存储后必须能把 cookie 会话合并回来，否则 OJ 登录态丢失。
        let dir = tempfile::tempdir().unwrap();
        let store = crate::storage::vault::Vault::open(dir.path().to_path_buf(), [3u8; 32]);
        let raw = r#"{"szu-1":{"username":"alice","password":"secret","url":"http://example/","cookies":"session=abc"}}"#;
        store
            .update(|document| set_raw(document, crate::storage::SecretKey::OjAccounts, raw))
            .unwrap();
        let loaded = load_oj_accounts(&store).unwrap().unwrap();
        let secrets: BTreeMap<String, OjSecret> = serde_json::from_str(&loaded).unwrap();
        assert_eq!(secrets["szu-1"].cookies, "session=abc");
        assert_eq!(secrets["szu-1"].username, "alice");
    }

    #[test]
    fn oj_accounts_without_cookies_roundtrip_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::storage::vault::Vault::open(dir.path().to_path_buf(), [4u8; 32]);
        let raw = r#"{"szu-1":{"username":"alice","password":"secret","url":"http://example/","cookies":""}}"#;
        store
            .update(|document| set_raw(document, crate::storage::SecretKey::OjAccounts, raw))
            .unwrap();
        let loaded = load_oj_accounts(&store).unwrap().unwrap();
        let secrets: BTreeMap<String, OjSecret> = serde_json::from_str(&loaded).unwrap();
        assert_eq!(secrets["szu-1"].cookies, "");
    }

    #[test]
    fn legacy_pending_and_monitor_are_preserved() {
        let mut document = VaultDocument::default();
        insert_legacy(
            &mut document,
            "pending-auth",
            r#"{"student_id":"20230001","password":"pw","name":"测试"}"#,
        )
        .unwrap();
        insert_legacy(
            &mut document,
            "electricity-monitor",
            r#"{"cache":null,"last_startup_attempt_date":null,"last_background_attempt_date":null,"last_notified_date":null}"#,
        )
        .unwrap();
        assert!(document.entries.contains_key(PENDING_AUTH));
        assert!(document.entries.contains_key(ELECTRICITY_MONITOR));
        document.validate().unwrap();
    }
}
