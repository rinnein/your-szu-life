//! 可同步项的本地读写与题库行键。
//!
//! 跨设备合并由 Loro 文档负责；这里只负责从各模块的存储读取当前值、把解析后的
//! 值写回各模块的**正常保存入口**，从而触发运行时更新、缓存失效与事件广播。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::AppHandle;

use szu_core::campus_network::model::DormCustomCredentials;
use szu_core::campus_payments::model::ElectricityLocationSettings;
use szu_core::storage::SharedCredentialStore;
use szu_uooc::uooc::model::Settings as UoocSettings;
use szu_uooc::uooc::quiz::qa::{LocalBankQuestion, SubmittedPaperRecord};

pub use szu_core::settings_sync_items::{
    DORM_CREDENTIALS, ELECTRICITY_LOCATION, OJ_ACCOUNTS, STAFF_CREDENTIALS, UOOC_AI, UOOC_QA_BANK,
    UOOC_SESSION, UOOC_SETTINGS,
};

#[derive(Clone, Copy, Debug)]
pub struct ItemDef {
    pub id: &'static str,
    pub label: &'static str,
    /// 集合项按行存放在独立的 Loro 子 map 中。
    pub collection: bool,
}

pub const ITEM_DEFS: [ItemDef; 8] = [
    ItemDef {
        id: UOOC_SESSION,
        label: "UOOC 登录态",
        collection: false,
    },
    ItemDef {
        id: UOOC_AI,
        label: "UOOC AI 配置",
        collection: false,
    },
    ItemDef {
        id: OJ_ACCOUNTS,
        label: "OJ 账号密码",
        collection: false,
    },
    ItemDef {
        id: DORM_CREDENTIALS,
        label: "宿舍区校园网账号",
        collection: false,
    },
    ItemDef {
        id: STAFF_CREDENTIALS,
        label: "教工区校园网账号",
        collection: false,
    },
    ItemDef {
        id: ELECTRICITY_LOCATION,
        label: "宿舍信息",
        collection: false,
    },
    ItemDef {
        id: UOOC_SETTINGS,
        label: "UOOC 运行设置",
        collection: false,
    },
    ItemDef {
        id: UOOC_QA_BANK,
        label: "UOOC 题库",
        collection: true,
    },
];

pub fn item_def(id: &str) -> Option<ItemDef> {
    ITEM_DEFS.iter().copied().find(|def| def.id == id)
}

#[derive(Clone)]
pub struct ItemContext {
    pub app: AppHandle,
    pub credentials: SharedCredentialStore,
}

impl ItemContext {
    pub async fn read(&self, id: &str) -> Result<Option<Value>, String> {
        match id {
            UOOC_SESSION => Ok(self.credentials.load_uooc_session()?.map(Value::String)),
            UOOC_AI => Ok(self.credentials.load_uooc_ai()?.map(Value::String)),
            OJ_ACCOUNTS => {
                let Some(raw) = self.credentials.load_oj_accounts()? else {
                    return Ok(None);
                };
                let secrets: BTreeMap<String, OjStored> = serde_json::from_str(&raw)
                    .map_err(|_| "OJ 账号数据损坏，已跳过同步".to_string())?;
                let accounts: Map<String, Value> = secrets
                    .into_iter()
                    .map(|(site, account)| {
                        (
                            site,
                            json!({
                                "username": account.username,
                                "password": account.password,
                                "url": account.url,
                            }),
                        )
                    })
                    .collect();
                Ok(Some(Value::Object(accounts)))
            }
            DORM_CREDENTIALS => to_value(self.credentials.load_dorm_credentials()?),
            STAFF_CREDENTIALS => to_value(self.credentials.load_staff_credentials()?),
            ELECTRICITY_LOCATION => to_value(self.credentials.load_electricity_settings()?),
            UOOC_SETTINGS => {
                let settings = szu_uooc::uooc::synced_settings(&self.app)?;
                Ok(Some(
                    serde_json::to_value(settings)
                        .map_err(|error| format!("UOOC 运行设置序列化失败: {error}"))?,
                ))
            }
            _ => Err(format!("未知的同步项: {id}")),
        }
    }

    pub async fn write(&self, id: &str, value: Value) -> Result<(), String> {
        match id {
            UOOC_SESSION => {
                let raw = expect_string(id, value)?;
                self.credentials.save_uooc_session(&raw)?;
                szu_uooc::uooc::reload_synced_session(&self.app)?;
                Ok(())
            }
            UOOC_AI => {
                let raw = expect_string(id, value)?;
                self.credentials.save_uooc_ai(&raw)?;
                szu_uooc::uooc::reload_synced_ai(&self.app)?;
                Ok(())
            }
            OJ_ACCOUNTS => {
                self.write_oj_accounts(value)?;
                szu_oj::oj::reload_synced_accounts(&self.app)
            }
            DORM_CREDENTIALS => {
                let credentials: DormCustomCredentials = serde_json::from_value(value)
                    .map_err(|_| "宿舍区校园网账号格式无效".to_string())?;
                szu_core::campus_network::service::apply_synced_dorm_credentials(
                    self.app.clone(),
                    credentials,
                )
                .map(|_| ())
            }
            STAFF_CREDENTIALS => {
                let credentials: DormCustomCredentials = serde_json::from_value(value)
                    .map_err(|_| "教工区校园网账号格式无效".to_string())?;
                szu_core::campus_network::staff_login::apply_synced_credentials(
                    self.app.clone(),
                    credentials,
                )
                .map(|_| ())
            }
            ELECTRICITY_LOCATION => {
                let settings: ElectricityLocationSettings = serde_json::from_value(value)
                    .map_err(|_| "宿舍信息格式无效".to_string())?;
                szu_core::campus_payments::electricity::apply_synced_settings(&self.app, settings)
            }
            UOOC_SETTINGS => {
                let settings: UoocSettings = serde_json::from_value(value)
                    .map_err(|_| "UOOC 运行设置格式无效".to_string())?;
                szu_uooc::uooc::apply_synced_settings(&self.app, settings)
            }
            _ => Err(format!("未知的同步项: {id}")),
        }
    }

    /// OJ 按完整快照应用：对端删除的账号在本机也删除，仍存在的站点保留本机 cookie。
    fn write_oj_accounts(&self, value: Value) -> Result<(), String> {
        let incoming: BTreeMap<String, OjAccount> =
            serde_json::from_value(value).map_err(|_| "OJ 账号格式无效".to_string())?;
        let existing: BTreeMap<String, OjStored> = self
            .credentials
            .load_oj_accounts()?
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        let mut secrets = BTreeMap::new();
        for (site, account) in incoming {
            let cookies = existing
                .get(&site)
                .map(|stored| stored.cookies.clone())
                .unwrap_or_default();
            secrets.insert(
                site,
                OjStored {
                    username: account.username,
                    password: account.password,
                    url: account.url,
                    cookies,
                },
            );
        }
        let raw = serde_json::to_string(&secrets)
            .map_err(|error| format!("OJ 账号序列化失败: {error}"))?;
        self.credentials.save_oj_accounts(&raw)
    }

    pub async fn bank_snapshot(&self) -> Result<BankSnapshot, String> {
        let (questions, papers) = szu_uooc::uooc::qa_sync_snapshot(&self.app).await?;
        Ok(BankSnapshot { questions, papers })
    }

    pub async fn apply_questions(&self, questions: Vec<LocalBankQuestion>) -> Result<(), String> {
        szu_uooc::uooc::qa_sync_apply_questions(&self.app, questions).await
    }

    pub async fn apply_papers(&self, papers: Vec<SubmittedPaperRecord>) -> Result<(), String> {
        szu_uooc::uooc::qa_sync_apply_papers(&self.app, papers).await
    }
}

#[derive(Clone, Debug, Default)]
pub struct BankSnapshot {
    pub questions: Vec<LocalBankQuestion>,
    pub papers: Vec<SubmittedPaperRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OjStored {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    cookies: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OjAccount {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    url: String,
}

fn expect_string(id: &str, value: Value) -> Result<String, String> {
    match value {
        Value::String(value) => Ok(value),
        _ => Err(format!("{id} 的内容必须是字符串")),
    }
}

fn to_value<T: Serialize>(value: Option<T>) -> Result<Option<Value>, String> {
    value
        .map(|value| {
            serde_json::to_value(value)
                .map_err(|error| format!("同步内容序列化失败: {error}"))
        })
        .transpose()
}

pub fn question_row_key(question: &LocalBankQuestion) -> String {
    format!("q:{}", question.question_id)
}

pub fn paper_row_key(paper: &SubmittedPaperRecord) -> String {
    format!(
        "p:{}:{}",
        paper.cid,
        szu_uooc::uooc::quiz::qa::sanitize_task_id(&paper.tid)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use szu_uooc::uooc::quiz::model::Question;

    #[test]
    fn item_defs_are_unique_and_cover_the_whitelist() {
        let mut ids: Vec<_> = ITEM_DEFS.iter().map(|def| def.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ITEM_DEFS.len());
        assert_eq!(ids.len(), 8);
        assert!(item_def(UOOC_QA_BANK).unwrap().collection);
        assert!(!item_def(OJ_ACCOUNTS).unwrap().collection);
        assert!(item_def("fixed.account.profile").is_none());
    }

    #[test]
    fn row_keys_are_stable_and_prefixed() {
        let question = LocalBankQuestion {
            cid: "c1".into(),
            course_name: None,
            tid: "t1".into(),
            task_name: None,
            question_id: "q1".into(),
            question_type: "single".into(),
            stem: None,
            options: None,
            blanks: 0,
            answers: Vec::new(),
            correct_answers: Vec::new(),
            images: Vec::new(),
            uploaded: false,
            updated_at_ms: 1,
        };
        assert_eq!(question_row_key(&question), "q:q1");
        let paper = SubmittedPaperRecord {
            cid: "c".into(),
            tid: " t/1 ".into(),
            task_title: None,
            questions: Vec::<Question>::new(),
            answers: Default::default(),
            submitted_at_ms: 1,
        };
        assert_eq!(
            paper_row_key(&paper),
            format!(
                "p:c:{}",
                szu_uooc::uooc::quiz::qa::sanitize_task_id(" t/1 ")
            )
        );
    }
}
