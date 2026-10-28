use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use super::model::{Answers, Question};
use super::qa::{hash_stem, LocalBankAnswer, LocalBankQuestion, QaUploadOption, SubmittedPaperRecord};

/// 视频时长模型 (Toasty Model)
#[derive(Debug, toasty::Model)]
pub struct VideoDuration {
    #[key]
    pub id: String, // format!("{cid}:{resource}")
    pub duration: f64,
    pub updated_at_ms: i64,
}

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
        let db = crate::migrations::db::open_sqlite_db(
            db_path,
            toasty::models!(SubmittedPaper, BankQuestion, VideoDuration),
            Some(&crate::migrations::uooc::UOOC_MIGRATIONS),
        )
        .await?;
        Ok(Self { db })
    }

    /// 保存试卷最后一次提交快照（覆盖写入）
    pub async fn save_submitted_paper(&self, record: &SubmittedPaperRecord) -> Result<(), String> {
        let key = format!("{}:{}", record.cid, record.tid);
        let q_json = serde_json::to_string(&record.questions).unwrap_or_default();
        let a_json = serde_json::to_string(&record.answers).unwrap_or_default();

        let mut db = self.db.clone();
        match SubmittedPaper::get_by_id(&mut db, &key).await {
            Ok(mut existing) => {
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
            }
            Err(error) if error.is_record_not_found() => {
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
            Err(error) => {
                return Err(format!("读取提交试卷记录失败，本次未写入: {error}"));
            }
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
            match SubmittedPaper::get_by_id(&mut db, &key).await {
                Ok(row) => return Ok(Some(self.row_to_submitted_paper_record(&row))),
                Err(error) if error.is_record_not_found() => {}
                Err(error) => {
                    // 读失败时不要继续走 tid 兜底：那可能返回旧试卷并覆盖当前试卷。
                    return Err(format!("读取提交试卷记录失败: {error}"));
                }
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

        let clean_opts: Option<Vec<QaUploadOption>> = q.options.as_ref().map(|opts| {
            opts.iter().map(|o| QaUploadOption { id: o.id.clone(), text: o.text.clone() }).collect()
        });
        let embed_text = super::qa::build_embedding_text(Some(&stem), clean_opts.as_deref());
        let vec_f32 = super::qa::compute_local_embedding(&embed_text);
        let computed_blob = super::qa::vector_to_blob(&vec_f32);

        let mut db = self.db.clone();
        match BankQuestion::get_by_question_id(&mut db, &q.question_id).await {
            Ok(mut existing) => {
                let next_cid = cid.map(str::to_string).or_else(|| existing.cid.clone());
                let next_tid = tid.map(str::to_string).or_else(|| existing.tid.clone());
                let next_cname = q.course_name.clone().or_else(|| existing.course_name.clone());
                let next_tname = q.task_name.clone().or_else(|| existing.task_name.clone());
                let embedding_blob = existing.embedding_blob.clone().or(Some(computed_blob));
                // 重建题库时新解析经常拿不到标准答案/作答记录；这些字段必须沿用旧值，
                // 否则每次打开页面重建都会把已经积累的答案抹掉。
                let next_correct =
                    correct_answers_json.or_else(|| existing.correct_answers_json.clone());
                let next_answers = if q.answers.is_empty() {
                    existing.answers_json.clone()
                } else {
                    answers_json
                };
                let next_options = options_json.or_else(|| existing.options_json.clone());
                let next_stem = if stem.is_empty() {
                    existing.stem.clone()
                } else {
                    stem
                };
                let next_stem_hash = if next_stem.is_empty() {
                    existing.stem_hash.clone()
                } else {
                    stem_h
                };
                let was_uploaded = existing.uploaded;
                let next_updated_at = q.updated_at_ms.max(existing.updated_at_ms);

                toasty::update!(existing {
                    cid: next_cid,
                    tid: next_tid,
                    course_name: next_cname,
                    task_name: next_tname,
                    question_type: q.question_type.clone(),
                    stem: next_stem,
                    stem_hash: next_stem_hash,
                    options_json: next_options,
                    answers_json: next_answers,
                    correct_answers_json: next_correct,
                    blanks: q.blanks as i64,
                    uploaded: was_uploaded || q.uploaded,
                    embedding_blob,
                    updated_at_ms: next_updated_at,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("更新题库题目失败: {e}"))?;
            }
            Err(error) if error.is_record_not_found() => {
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
                    embedding_blob: Some(computed_blob),
                    updated_at_ms: q.updated_at_ms,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("新增题库题目失败: {e}"))?;
            }
            Err(error) => {
                return Err(format!("读取题库题目失败，本次未写入: {error}"));
            }
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

    /// 读取全部视频时长缓存
    pub async fn load_all_video_durations(&self) -> Result<HashMap<String, f64>, String> {
        let mut db = self.db.clone();
        let rows = VideoDuration::all()
            .exec(&mut db)
            .await
            .map_err(|e| format!("读取视频时长缓存失败: {e}"))?;
        let mut map = HashMap::new();
        for r in rows {
            if r.duration.is_finite() && r.duration > 1.0 {
                map.insert(r.id, r.duration);
            }
        }
        Ok(map)
    }

    /// 保存单条视频时长
    pub async fn save_video_duration(&self, cid: &str, resource: &str, duration: f64) -> Result<(), String> {
        let key = format!("{cid}:{resource}");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let mut db = self.db.clone();
        match VideoDuration::get_by_id(&mut db, &key).await {
            Ok(mut existing) => {
                toasty::update!(existing {
                    duration,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("更新视频时长失败: {e}"))?;
            }
            Err(error) if error.is_record_not_found() => {
                toasty::create!(VideoDuration {
                    id: key,
                    duration,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("保存视频时长失败: {e}"))?;
            }
            Err(error) => {
                return Err(format!("读取视频时长失败，本次未写入: {error}"));
            }
        }
        Ok(())
    }

    /// 标记题目为已上传
    pub async fn mark_questions_uploaded(&self, question_ids: &[String]) -> Result<usize, String> {
        let mut count = 0;
        let mut db = self.db.clone();
        for qid in question_ids {
            match BankQuestion::get_by_question_id(&mut db, qid).await {
                Ok(mut existing) => {
                    if !existing.uploaded {
                        toasty::update!(existing { uploaded: true })
                            .exec(&mut db)
                            .await
                            .map_err(|e| format!("标记题库上传状态失败: {e}"))?;
                        count += 1;
                    }
                }
                Err(error) if error.is_record_not_found() => {}
                Err(error) => {
                    return Err(format!("读取题库题目失败: {error}"));
                }
            }
        }
        Ok(count)
    }

    /// 统计拥有特征向量的题目数量
    pub async fn count_embedding_questions(&self) -> Result<usize, String> {
        let mut db = self.db.clone();
        let rows = BankQuestion::all()
            .exec(&mut db)
            .await
            .map_err(|e| format!("查询题目失败: {e}"))?;
        Ok(rows.into_iter().filter(|r| r.embedding_blob.is_some()).count())
    }

    /// 读取所有带向量特征的题目
    pub async fn load_all_embedded_questions(&self) -> Result<Vec<(LocalBankQuestion, Vec<f32>)>, String> {
        let mut db = self.db.clone();
        let rows = BankQuestion::all()
            .exec(&mut db)
            .await
            .map_err(|e| format!("查询全部题目向量失败: {e}"))?;
        let mut res = Vec::new();
        for r in rows {
            let Some(ref blob) = r.embedding_blob else { continue };
            let vec_f32 = super::qa::blob_to_vector(blob);
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
            res.push((q_item, vec_f32));
        }
        Ok(res)
    }

    /// 构建内存向量检索库 (用于 AI RAG)
    pub async fn build_in_memory_rag_store(
        &self,
        cid: Option<&str>,
        tid: Option<&str>,
    ) -> Result<super::qa::InMemoryVectorStore<super::qa::QuestionRagDocument>, String> {
        let embedded = self.load_all_embedded_questions().await?;
        let mut store = super::qa::InMemoryVectorStore::default();
        let formatted = embedded
            .into_iter()
            .filter(|(q, _)| {
                if let Some(c) = cid {
                    if q.cid != c { return false; }
                }
                if let Some(t) = tid {
                    if q.tid != t { return false; }
                }
                true
            })
            .map(|(q, v_f32)| {
                let options = q.options
                    .unwrap_or_default()
                    .into_iter()
                    .map(|o| format!("{}. {}", o.id, o.text))
                    .collect();
                let doc = super::qa::QuestionRagDocument {
                    question_id: q.question_id.clone(),
                    tid: if q.tid.is_empty() { None } else { Some(q.tid) },
                    cid: if q.cid.is_empty() { None } else { Some(q.cid) },
                    stem: q.stem.unwrap_or_default(),
                    question_type: q.question_type,
                    options,
                    answers: q.correct_answers,
                };
                let embed_text = doc.to_embed_text();
                let vec_f64: Vec<f64> = v_f32.into_iter().map(|x| x as f64).collect();
                let qid = doc.question_id.clone();
                (qid, doc, vec![rig_core::embeddings::Embedding { document: embed_text, vec: vec_f64 }])
            });
        store.add_documents_with_ids(formatted);
        Ok(store)
    }

    /// 录入服务端拉取的题目与向量特征
    pub async fn upsert_server_question(
        &self,
        cid: Option<&str>,
        tid: &str,
        question_id: &str,
        question_type: &str,
        stem: &str,
        options: Option<&[QaUploadOption]>,
        answers: &[String],
        embedding: &[f32],
    ) -> Result<(), String> {
        let stem_h = hash_stem(stem);
        let options_json = options.map(|opts| serde_json::to_string(opts).unwrap_or_default());
        let correct_answers_json = if answers.is_empty() {
            None
        } else {
            Some(serde_json::to_string(answers).unwrap_or_default())
        };
        let embedding_blob = super::qa::vector_to_blob(embedding);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let mut db = self.db.clone();
        match BankQuestion::get_by_question_id(&mut db, question_id).await {
            Ok(mut existing) => {
                let next_cid = cid.map(str::to_string).or_else(|| existing.cid.clone());
                let next_tid = Some(tid.to_string()).or_else(|| existing.tid.clone());
                let next_correct =
                    correct_answers_json.or_else(|| existing.correct_answers_json.clone());
                let next_options = options_json.or_else(|| existing.options_json.clone());
                toasty::update!(existing {
                    cid: next_cid,
                    tid: next_tid,
                    question_type: question_type.to_string(),
                    stem: stem.to_string(),
                    stem_hash: stem_h,
                    options_json: next_options,
                    correct_answers_json: next_correct,
                    embedding_blob: Some(embedding_blob),
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("更新服务端拉取题目失败: {e}"))?;
            }
            Err(error) if error.is_record_not_found() => {
                toasty::create!(BankQuestion {
                    question_id: question_id.to_string(),
                    cid: cid.map(str::to_string),
                    tid: Some(tid.to_string()),
                    course_name: None,
                    task_name: None,
                    question_type: question_type.to_string(),
                    stem: stem.to_string(),
                    stem_hash: stem_h,
                    options_json,
                    answers_json: "[]".to_string(),
                    correct_answers_json,
                    blanks: 0,
                    uploaded: true,
                    embedding_blob: Some(embedding_blob),
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("新增服务端拉取题目失败: {e}"))?;
            }
            Err(error) => {
                return Err(format!("读取题库题目失败，本次未写入: {error}"));
            }
        }
        Ok(())
    }

    /// 清空题库所有题目
    pub async fn clear_all_questions(&self) -> Result<usize, String> {
        let mut db = self.db.clone();
        let rows = BankQuestion::all()
            .exec(&mut db)
            .await
            .map_err(|e| format!("查询全部题目失败: {e}"))?;
        let count = rows.len();
        for r in rows {
            let _ = r.delete().exec(&mut db).await;
        }
        Ok(count)
    }
}
