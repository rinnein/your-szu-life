//! 可同步项的读写与内容哈希。
//!
//! 白名单就是这里的 [`ITEM_DEFS`]：只有它们会出现在同步请求里。每一项都通过各
//! 模块已有的存储接口读写，绝不直接改写 vault 文件或别人的数据库；写完后按需要
//! 触发对应模块的运行时刷新。
//!
//! 题库是唯一的集合项：题目按 `question_id`、试卷按 `cid:clean_tid` 逐行合并，
//! 因此这里额外提供行键与行哈希。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tauri::AppHandle;

use szu_core::campus_payments::model::ElectricityLocationSettings;
use szu_core::campus_network::model::DormCustomCredentials;
use szu_core::storage::SharedCredentialStore;
use szu_uooc::uooc::model::Settings as UoocSettings;
use szu_uooc::uooc::quiz::qa::{LocalBankQuestion, SubmittedPaperRecord};

pub const UOOC_SESSION: &str = "uooc.session";
pub const UOOC_AI: &str = "uooc.ai";
pub const OJ_ACCOUNTS: &str = "oj.accounts";
pub const DORM_CREDENTIALS: &str = "network.dorm.credentials";
pub const STAFF_CREDENTIALS: &str = "network.staff.credentials";
pub const ELECTRICITY_LOCATION: &str = "electricity.location";
pub const UOOC_SETTINGS: &str = "uooc.settings";
pub const UOOC_QA_BANK: &str = "uooc.qa_bank";

#[derive(Clone, Copy, Debug)]
pub struct ItemDef {
    pub id: &'static str,
    pub label: &'static str,
    /// 集合项不参与整项版本向量合并，走行级合并。
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
            UOOC_SESSION => Ok(self
                .credentials
                .load_uooc_session()?
                .map(Value::String)),
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
            OJ_ACCOUNTS => self.write_oj_accounts(value),
            DORM_CREDENTIALS => {
                let credentials: DormCustomCredentials = serde_json::from_value(value)
                    .map_err(|_| "宿舍区校园网账号格式无效".to_string())?;
                self.credentials.save_dorm_credentials(&credentials)
            }
            STAFF_CREDENTIALS => {
                let credentials: DormCustomCredentials = serde_json::from_value(value)
                    .map_err(|_| "教工区校园网账号格式无效".to_string())?;
                self.credentials.save_staff_credentials(&credentials)
            }
            ELECTRICITY_LOCATION => {
                let settings: ElectricityLocationSettings = serde_json::from_value(value)
                    .map_err(|_| "宿舍信息格式无效".to_string())?;
                self.credentials.save_electricity_settings(&settings)
            }
            UOOC_SETTINGS => {
                let settings: UoocSettings = serde_json::from_value(value)
                    .map_err(|_| "UOOC 运行设置格式无效".to_string())?;
                szu_uooc::uooc::apply_synced_settings(&self.app, settings)
            }
            _ => Err(format!("未知的同步项: {id}")),
        }
    }

    /// OJ 只同步账号与密码；本机的 cookie 会话必须保留，不能被对端覆盖。
    fn write_oj_accounts(&self, value: Value) -> Result<(), String> {
        let incoming: BTreeMap<String, OjAccount> =
            serde_json::from_value(value).map_err(|_| "OJ 账号格式无效".to_string())?;
        let mut secrets: BTreeMap<String, OjStored> = self
            .credentials
            .load_oj_accounts()?
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        for (site, account) in incoming {
            let cookies = secrets
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

    pub async fn apply_questions(
        &self,
        questions: Vec<LocalBankQuestion>,
    ) -> Result<(), String> {
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
        .map(|value| serde_json::to_value(value).map_err(|error| format!("同步内容序列化失败: {error}")))
        .transpose()
}

/// 整项内容哈希。值本身已经是稳定的 JSON 结构。
pub fn content_hash(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    sha256_hex(&bytes)
}

pub fn question_row_key(question: &LocalBankQuestion) -> String {
    format!("q:{}", question.question_id)
}

pub fn paper_row_key(paper: &SubmittedPaperRecord) -> String {
    format!("p:{}:{}", paper.cid, szu_uooc::uooc::quiz::qa::sanitize_task_id(&paper.tid))
}

/// 题目行哈希。选项、作答顺序可能因设备不同而不同，这里先做规范化再哈希。
pub fn question_hash(question: &LocalBankQuestion) -> String {
    let mut answers = question.answers.clone();
    answers.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| format!("{:?}", a.values).cmp(&format!("{:?}", b.values)))
            .then_with(|| a.is_correct.cmp(&b.is_correct))
    });
    let mut options = question.options.clone().unwrap_or_default();
    options.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.text.cmp(&b.text)));
    let canonical = json!({
        "cid": question.cid,
        "tid": question.tid,
        "questionId": question.question_id,
        "type": question.question_type,
        "stem": question.stem,
        "options": options,
        "blanks": question.blanks,
        "answers": answers,
        "correctAnswers": question.correct_answers,
        "uploaded": question.uploaded,
    });
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    sha256_hex(&bytes)
}

pub fn paper_hash(paper: &SubmittedPaperRecord) -> String {
    let bytes = serde_json::to_vec(paper).unwrap_or_default();
    sha256_hex(&bytes)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use szu_uooc::uooc::quiz::model::Question;
    use szu_uooc::uooc::quiz::qa::{LocalBankAnswer, QaUploadOption};

    fn question(stem: &str, answers: Vec<LocalBankAnswer>, uploaded: bool) -> LocalBankQuestion {
        LocalBankQuestion {
            cid: "c1".into(),
            course_name: None,
            tid: "t1".into(),
            task_name: None,
            question_id: "q1".into(),
            question_type: "single".into(),
            stem: Some(stem.into()),
            options: Some(vec![
                QaUploadOption {
                    id: "B".into(),
                    text: "b".into(),
                },
                QaUploadOption {
                    id: "A".into(),
                    text: "a".into(),
                },
            ]),
            blanks: 0,
            answers,
            correct_answers: vec!["A".into()],
            uploaded,
            updated_at_ms: 1,
        }
    }

    fn answer(id: i64) -> LocalBankAnswer {
        LocalBankAnswer {
            id: Some(id),
            values: vec![serde_json::json!("A")],
            is_correct: true,
            source: "sync".into(),
            confidence: 1.0,
        }
    }

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
    fn question_hash_ignores_option_and_answer_order() {
        let a = question("stem", vec![answer(2), answer(1)], false);
        let b = question("stem", vec![answer(1), answer(2)], false);
        assert_eq!(question_hash(&a), question_hash(&b));
    }

    #[test]
    fn question_hash_changes_with_content_or_upload_state() {
        let base = question("stem", vec![answer(1)], false);
        assert_ne!(question_hash(&base), question_hash(&question("other", vec![answer(1)], false)));
        assert_ne!(question_hash(&base), question_hash(&question("stem", vec![answer(2)], false)));
        assert_ne!(question_hash(&base), question_hash(&question("stem", vec![answer(1)], true)));
    }

    #[test]
    fn paper_hash_changes_with_answers() {
        let mut paper = SubmittedPaperRecord {
            cid: "c".into(),
            tid: "t".into(),
            task_title: None,
            questions: Vec::<Question>::new(),
            answers: Default::default(),
            submitted_at_ms: 1,
        };
        let first = paper_hash(&paper);
        paper.answers.insert("q1".into(), vec!["A".into()]);
        assert_ne!(first, paper_hash(&paper));
    }

    #[test]
    fn row_keys_are_stable_and_prefixed() {
        let q = question("stem", vec![], false);
        assert_eq!(question_row_key(&q), "q:q1");
        let paper = SubmittedPaperRecord {
            cid: "c".into(),
            tid: " t/1 ".into(),
            task_title: None,
            questions: Vec::new(),
            answers: Default::default(),
            submitted_at_ms: 1,
        };
        assert_eq!(paper_row_key(&paper), format!("p:c:{}", szu_uooc::uooc::quiz::qa::sanitize_task_id(" t/1 ")));
    }
}
