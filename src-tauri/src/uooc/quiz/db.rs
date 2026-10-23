use std::collections::BTreeMap;
use std::path::Path;
use super::model::{Answers, Question};
use super::qa::{hash_stem, LocalBankAnswer, LocalBankQuestion, QaUploadOption, SubmittedPaperRecord};

/// 试卷最后一次提交快照模型 (Toasty Model)
#[derive(Debug, toasty::Model)]
pub struct SubmittedPaper {
    #[key]
    pub id: String, // 复合键：format!("{cid}:{clean_tid}")
    #[index]
    pub cid: String,
    #[index]
    pub tid: String,
    pub task_title: Option<String>,
    pub questions_json: String,
    pub answers_json: String,
    pub submitted_at_ms: i64,
}

/// 题库题目模型 (Toasty Model)
#[derive(Debug, toasty::Model)]
pub struct BankQuestion {
    #[key]
    pub question_id: String,
    #[index]
    pub cid: Option<String>,
    #[index]
    pub tid: Option<String>,
    pub course_name: Option<String>,
    pub task_name: Option<String>,
    pub question_type: String,
    pub stem: String,
    #[index]
    pub stem_hash: String,
    pub options_json: Option<String>,
    pub answers_json: String,
    pub correct_answers_json: Option<String>,
    pub blanks: i64,
    pub uploaded: bool,
    pub embedding_blob: Option<Vec<u8>>,
    pub updated_at_ms: i64,
}

/// 异步 Toasty ORM 题库数据库封装
#[derive(Clone)]
pub struct QaDatabase {
    pub(crate) db: toasty::Db,
}

impl QaDatabase {
    pub async fn open(db_path: &Path) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let url = format!("sqlite://{}", db_path.display());
        let db = toasty::Db::builder()
            .models(toasty::models!(SubmittedPaper, BankQuestion))
            .connect(&url)
            .await
            .map_err(|e| format!("打开 Toasty 题库数据库失败: {e}"))?;

        db.push_schema()
            .await
            .map_err(|e| format!("初始化 Toasty 题库表结构失败: {e}"))?;

        Ok(Self { db })
    }

    /// 保存试卷最后一次提交快照（覆盖写入）
    pub async fn save_submitted_paper(&self, record: &SubmittedPaperRecord) -> Result<(), String> {
        let key = format!("{}:{}", record.cid, record.tid);
        let q_json = serde_json::to_string(&record.questions).unwrap_or_default();
        let a_json = serde_json::to_string(&record.answers).unwrap_or_default();

        let mut db = self.db.clone();
        // 尝试查找旧记录
        if let Ok(mut existing) = SubmittedPaper::get_by_id(&mut db, &key).await {
            toasty::update!(existing {
                cid: record.cid.clone(),
                tid: record.tid.clone(),
                task_title: record.task_title.clone(),
                questions_json: q_json,
                answers_json: a_json,
                submitted_at_ms: record.submitted_at_ms,
            })
            .exec(&mut db)
            .await
            .map_err(|e| format!("更新提交试卷记录失败: {e}"))?;
        } else {
            toasty::create!(SubmittedPaper {
                id: key,
                cid: record.cid.clone(),
                tid: record.tid.clone(),
                task_title: record.task_title.clone(),
                questions_json: q_json,
                answers_json: a_json,
                submitted_at_ms: record.submitted_at_ms,
            })
            .exec(&mut db)
            .await
            .map_err(|e| format!("创建提交试卷记录失败: {e}"))?;
        }
        Ok(())
    }

    /// 查询某题单的最后一次提交快照
    pub async fn get_submitted_paper(
        &self,
        cid: Option<&str>,
        tid: &str,
    ) -> Result<Option<SubmittedPaperRecord>, String> {
        let clean_tid = super::qa::sanitize_task_id(tid);
        let mut db = self.db.clone();

        // 1. 若提供 cid，直接按复合键 id 检索
        if let Some(c) = cid {
            let key = format!("{c}:{clean_tid}");
            if let Ok(row) = SubmittedPaper::get_by_id(&mut db, &key).await {
                return Ok(Some(self.row_to_submitted_paper_record(&row)));
            }
        }

        // 2. 若无 cid 或精确匹配未找到，按 tid 索引检索
        let list = SubmittedPaper::filter_by_tid(clean_tid)
            .exec(&mut db)
            .await
            .map_err(|e| format!("按 tid 查询提交试卷记录失败: {e}"))?;

        let mut sorted = list.into_iter().collect::<Vec<_>>();
        sorted.sort_by(|a, b| b.submitted_at_ms.cmp(&a.submitted_at_ms));

        if let Some(row) = sorted.first() {
            return Ok(Some(self.row_to_submitted_paper_record(row)));
        }

        Ok(None)
    }

    /// 查询某课程的所有提交试卷
    pub async fn load_all_submitted_papers_for_course(
        &self,
        cid: &str,
    ) -> Result<Vec<SubmittedPaperRecord>, String> {
        let mut db = self.db.clone();
        let list = SubmittedPaper::filter_by_cid(cid)
            .exec(&mut db)
            .await
            .map_err(|e| format!("按 cid 查询提交试卷列表失败: {e}"))?;

        Ok(list
            .into_iter()
            .map(|r| self.row_to_submitted_paper_record(&r))
            .collect())
    }

    fn row_to_submitted_paper_record(&self, row: &SubmittedPaper) -> SubmittedPaperRecord {
        let questions: Vec<Question> = serde_json::from_str(&row.questions_json).unwrap_or_default();
        let answers: Answers = serde_json::from_str(&row.answers_json).unwrap_or_default();
        SubmittedPaperRecord {
            cid: row.cid.clone(),
            tid: row.tid.clone(),
            task_title: row.task_title.clone(),
            questions,
            answers,
            submitted_at_ms: row.submitted_at_ms,
        }
    }

    /// 录入或更新题库单题
    pub async fn upsert_bank_question(
        &self,
        q: &LocalBankQuestion,
        cid: Option<&str>,
        tid: Option<&str>,
    ) -> Result<(), String> {
        let stem = q.stem.clone().unwrap_or_default();
        let stem_h = hash_stem(&stem);
        let options_json = q.options.as_ref().map(|opts| serde_json::to_string(opts).unwrap_or_default());
        let answers_json = serde_json::to_string(&q.answers).unwrap_or_default();
        let correct_answers_json = if q.correct_answers.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&q.correct_answers).unwrap_or_default())
        };

        let mut db = self.db.clone();
        if let Ok(mut existing) = BankQuestion::get_by_question_id(&mut db, &q.question_id).await {
            let next_cid = cid.map(str::to_string).or_else(|| existing.cid.clone());
            let next_tid = tid.map(str::to_string).or_else(|| existing.tid.clone());
            let next_cname = q.course_name.clone().or_else(|| existing.course_name.clone());
            let next_tname = q.task_name.clone().or_else(|| existing.task_name.clone());

            toasty::update!(existing {
                cid: next_cid,
                tid: next_tid,
                course_name: next_cname,
                task_name: next_tname,
                question_type: q.question_type.clone(),
                stem,
                stem_hash: stem_h,
                options_json,
                answers_json,
                correct_answers_json,
                blanks: q.blanks as i64,
                uploaded: q.uploaded,
                updated_at_ms: q.updated_at_ms,
            })
            .exec(&mut db)
            .await
            .map_err(|e| format!("更新题库题目失败: {e}"))?;
        } else {
            toasty::create!(BankQuestion {
                question_id: q.question_id.clone(),
                cid: cid.map(str::to_string),
                tid: tid.map(str::to_string),
                course_name: q.course_name.clone(),
                task_name: q.task_name.clone(),
                question_type: q.question_type.clone(),
                stem,
                stem_hash: stem_h,
                options_json,
                answers_json,
                correct_answers_json,
                blanks: q.blanks as i64,
                uploaded: q.uploaded,
                embedding_blob: None,
                updated_at_ms: q.updated_at_ms,
            })
            .exec(&mut db)
            .await
            .map_err(|e| format!("新增题库题目失败: {e}"))?;
        }

        Ok(())
    }

    /// 获取题库总题数
    pub async fn count_questions(&self) -> Result<usize, String> {
        let mut db = self.db.clone();
        let all = BankQuestion::all()
            .exec(&mut db)
            .await
            .map_err(|e| format!("统计题库总数失败: {e}"))?;
        Ok(all.len())
    }

    /// 读取全部题库为 BTreeMap
    pub async fn load_all_bank_questions(&self) -> Result<BTreeMap<String, LocalBankQuestion>, String> {
        let mut db = self.db.clone();
        let rows = BankQuestion::all()
            .exec(&mut db)
            .await
            .map_err(|e| format!("读取全部题库题目失败: {e}"))?;

        let mut map = BTreeMap::new();
        for r in rows {
            let options: Option<Vec<QaUploadOption>> = r.options_json.as_deref().and_then(|s| serde_json::from_str(s).ok());
            let answers: Vec<LocalBankAnswer> = serde_json::from_str(&r.answers_json).unwrap_or_default();
            let correct_answers: Vec<String> = r.correct_answers_json.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();

            let q_item = LocalBankQuestion {
                cid: r.cid.clone().unwrap_or_default(),
                course_name: r.course_name.clone(),
                tid: r.tid.clone().unwrap_or_default(),
                task_name: r.task_name.clone(),
                question_id: r.question_id.clone(),
                question_type: r.question_type.clone(),
                stem: if r.stem.is_empty() { None } else { Some(r.stem.clone()) },
                options,
                blanks: r.blanks.max(0) as usize,
                answers,
                correct_answers,
                uploaded: r.uploaded,
                updated_at_ms: r.updated_at_ms,
            };

            let tid_part = r.tid.as_deref().unwrap_or_default();
            let key = super::qa::bank_key(tid_part, &r.question_id);
            map.insert(key, q_item);
        }

        Ok(map)
    }
}
