use super::model::{Answers, Kind, OptionItem, Question, Review};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

pub(crate) const EMBEDDING_DIMENSIONS: usize = 1536;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QaOption {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QaAnswer {
    #[serde(default)]
    pub id: Option<i64>,
    pub values: Vec<String>,
    #[serde(rename = "isCorrect")]
    pub is_correct: bool,
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QaQuestion {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub tid: Option<String>,
    #[serde(rename = "questionId")]
    pub question_id: Option<String>,
    #[serde(rename = "type")]
    pub question_type: Option<String>,
    pub stem: Option<String>,
    #[serde(rename = "stemHash")]
    pub stem_hash: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<QaOption>>,
    #[serde(default)]
    pub answers: Vec<QaAnswer>,
    #[serde(default, rename = "stemEmbedding")]
    pub stem_embedding: Option<Vec<f32>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QaTaskGroup {
    pub tid: String,
    #[serde(default)]
    pub cid: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QaTaskResponse {
    #[serde(rename = "taskGroup")]
    pub task_group: Option<QaTaskGroup>,
    #[serde(default)]
    pub questions: Vec<QaQuestion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct QaUploadOption {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct QaUploadQuestion {
    pub question_id: String,
    #[serde(rename = "type")]
    pub question_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<QaUploadOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blanks: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correct_answers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrong_answers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct QaBatchUploadPayload {
    pub tid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub course_name: Option<String>,
    pub questions: Vec<QaUploadQuestion>,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct QaUploadActionResult {
    pub question_id: String,
    pub action: String,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct QaBatchUploadResponse {
    pub ok: bool,
    #[serde(default)]
    pub results: Vec<QaUploadActionResult>,
}

#[derive(Clone, Debug, Deserialize)]
struct AnonymousKeyResponse {
    pub ok: bool,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalBankQuestion {
    pub cid: String,
    pub tid: String,
    pub question_id: String,
    pub question_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<QaUploadOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blanks: Option<usize>,
    pub correct_answers: Vec<String>,
    pub uploaded: bool,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BankMatchItem {
    pub tid: String,
    pub question_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BankMatchPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<BankMatchItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_question_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct MissingOnServerItem {
    pub cid: Option<String>,
    pub tid: Option<String>,
    #[serde(alias = "question_id")]
    pub question_id: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct BankMatchResponse {
    pub ok: bool,
    #[serde(default, alias = "missing_on_server")]
    pub missing_on_server: Vec<MissingOnServerItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CachedVectorQuestion {
    pub id: Option<i64>,
    pub tid: String,
    pub cid: Option<String>,
    pub question_id: String,
    pub question_type: String,
    pub stem: String,
    pub embedding: Vec<f32>,
    pub options: Option<Vec<QaUploadOption>>,
    pub answers: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmbeddingModelStatus {
    pub status: String,
    pub cached_count: usize,
    pub total_dimension: usize,
    pub updated_at_ms: i64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QaSearchResultItem {
    pub question_id: String,
    pub tid: Option<String>,
    pub cid: Option<String>,
    pub stem: String,
    pub question_type: String,
    pub similarity: f64,
    pub answers: Vec<String>,
    pub source: String,
}

/// Normalize stem according to uooc-qa-share specification:
/// strip HTML tags, strip HTML entities, collapse whitespace, trim, lowercase.
pub(crate) fn normalize_stem(raw: &str) -> String {
    let mut without_html = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            without_html.push(ch);
        }
    }

    let mut without_entities = String::with_capacity(without_html.len());
    let mut chars = without_html.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::new();
            let mut found_semicolon = false;
            while let Some(&next_ch) = chars.peek() {
                if next_ch == ';' {
                    chars.next();
                    found_semicolon = true;
                    break;
                } else if next_ch.is_alphabetic() && entity.len() < 10 {
                    entity.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if found_semicolon && !entity.is_empty() {
                without_entities.push(' ');
            } else {
                without_entities.push('&');
                without_entities.push_str(&entity);
            }
        } else {
            without_entities.push(ch);
        }
    }

    without_entities
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Compute SHA-256 hash of normalized stem.
pub(crate) fn hash_stem(raw: &str) -> String {
    let normalized = normalize_stem(raw);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Compute a deterministic 1536-dimensional L2-normalized semantic feature vector.
pub(crate) fn compute_local_embedding(text: &str) -> Vec<f32> {
    let normalized = normalize_stem(text);
    let mut vector = vec![0.0_f32; EMBEDDING_DIMENSIONS];
    if normalized.is_empty() {
        vector[0] = 1.0;
        return vector;
    }

    let chars: Vec<char> = normalized.chars().collect();
    let n = chars.len();

    // 1-gram, 2-gram, 3-gram feature projection
    for gram_len in 1..=3 {
        if n < gram_len {
            continue;
        }
        for i in 0..=(n - gram_len) {
            let sub: String = chars[i..i + gram_len].iter().collect();
            let mut hasher = Sha256::new();
            hasher.update(sub.as_bytes());
            let hash_bytes = hasher.finalize();

            // Project each n-gram into 4 independent positions with alternating weights
            for seed in 0..4 {
                let offset = seed * 8;
                let chunk = &hash_bytes[offset..offset + 4];
                let idx = u32::from_le_bytes(chunk.try_into().unwrap()) as usize % EMBEDDING_DIMENSIONS;
                let sign = if (hash_bytes[offset + 4] & 1) == 0 { 1.0_f32 } else { -1.0_f32 };
                let weight = match gram_len {
                    1 => 1.0_f32,
                    2 => 1.5_f32,
                    _ => 2.0_f32,
                };
                vector[idx] += sign * weight;
            }
        }
    }

    // L2 normalize vector
    let norm_sq: f32 = vector.iter().map(|v| v * v).sum();
    if norm_sq > 0.0 {
        let norm = norm_sq.sqrt();
        for val in &mut vector {
            *val /= norm;
        }
    } else {
        vector[0] = 1.0;
    }

    vector
}

/// Cosine similarity between two unit vectors (dot product).
pub(crate) fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    (dot as f64).clamp(-1.0, 1.0)
}

fn http_client() -> Result<reqwest::Client, String> {
    #[cfg(target_os = "android")]
    crate::platform_tls::initialize_platform_verifier()?;
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("无法构建 HTTP 客户端: {e}"))
}

/// Automatically acquire or verify an API key via /api/auth/anonymous-key.
pub(crate) async fn ensure_api_key(server_url: &str, stored_key: Option<&str>) -> Option<String> {
    if let Some(key) = stored_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let base = server_url.trim_end_matches('/');
    let url = format!("{base}/api/auth/anonymous-key");
    let client = http_client().ok()?;
    let resp = client.post(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let data = resp.json::<AnonymousKeyResponse>().await.ok()?;
    if data.ok {
        data.api_key
    } else {
        None
    }
}

/// Query QA platform for questions and known answers of a task group.
pub(crate) async fn query_task_answers(
    server_url: &str,
    api_key: Option<&str>,
    tid: &str,
    cid: Option<&str>,
) -> Result<Vec<QaQuestion>, String> {
    let base = server_url.trim_end_matches('/');
    let url = match cid {
        Some(cid) if !cid.is_empty() => format!("{base}/api/courses/{cid}/tasks/{tid}"),
        _ => format!("{base}/api/tasks/{tid}"),
    };

    let client = http_client()?;
    let mut req = client.get(&url);
    if let Some(key) = api_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            req = req.header("x-api-key", trimmed);
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求题库共享平台失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("题库请求失败，状态码: {}", resp.status()));
    }

    let data = resp
        .json::<QaTaskResponse>()
        .await
        .map_err(|e| format!("解析题库数据失败: {e}"))?;
    Ok(data.questions)
}

/// Upload question results in batch to /api/upload/batch.
pub(crate) async fn upload_batch(
    server_url: &str,
    api_key: Option<&str>,
    payload: &QaBatchUploadPayload,
) -> Result<QaBatchUploadResponse, String> {
    let base = server_url.trim_end_matches('/');
    let url = format!("{base}/api/upload/batch");

    let client = http_client()?;
    let mut req = client.post(&url).json(payload);
    if let Some(key) = api_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            req = req.header("x-api-key", trimmed);
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("上报题库数据失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("上报题库数据被拒绝，状态码: {}", resp.status()));
    }

    let data = resp
        .json::<QaBatchUploadResponse>()
        .await
        .map_err(|e| format!("解析上报结果失败: {e}"))?;
    Ok(data)
}

#[derive(Default)]
pub(crate) struct MatchResult {
    pub matched_answers: Answers,
    pub wrong_options: BTreeMap<String, Vec<String>>,
    pub reviews: Vec<Review>,
}

/// Match local paper questions with questions from the QA platform.
pub(crate) fn match_answers(
    qa_questions: &[QaQuestion],
    local_questions: &[Question],
) -> MatchResult {
    let mut result = MatchResult::default();

    for local_q in local_questions {
        let local_hash = hash_stem(&local_q.text);
        let local_norm = normalize_stem(&local_q.text);

        // Find best match in qa_questions:
        // 1. Direct question_id equality (or substring)
        // 2. stemHash equality
        // 3. normalized stem equality
        let matched = qa_questions.iter().find(|qa| {
            if let Some(ref qid) = qa.question_id {
                if qid == &local_q.id
                    || local_q.id.contains(qid.as_str())
                    || qid.contains(local_q.id.as_str())
                {
                    return true;
                }
            }
            if let Some(ref sh) = qa.stem_hash {
                if sh == &local_hash {
                    return true;
                }
            }
            if let Some(ref stem) = qa.stem {
                if normalize_stem(stem) == local_norm {
                    return true;
                }
            }
            false
        });

        let Some(qa) = matched else {
            continue;
        };

        // Extract correct answers
        let correct_ans = qa.answers.iter().find(|a| a.is_correct && !a.values.is_empty());
        if let Some(correct) = correct_ans {
            let mapped_values = map_option_values(&correct.values, &local_q.options);
            if !mapped_values.is_empty() || local_q.kind == Kind::Text {
                let ans = if local_q.kind == Kind::Text {
                    correct.values.clone()
                } else {
                    mapped_values
                };
                result.matched_answers.insert(local_q.id.clone(), ans.clone());
                result.reviews.push(Review {
                    question_id: local_q.id.clone(),
                    answers: ans,
                    probabilities: Default::default(),
                    message: "题库命中标准答案".into(),
                });
            }
        }

        // Extract wrong answers (pruning candidates)
        let wrongs: Vec<String> = qa
            .answers
            .iter()
            .filter(|a| !a.is_correct)
            .flat_map(|a| map_option_values(&a.values, &local_q.options))
            .collect();
        if !wrongs.is_empty() {
            let mut unique_wrongs = wrongs.clone();
            unique_wrongs.sort();
            unique_wrongs.dedup();
            result
                .wrong_options
                .insert(local_q.id.clone(), unique_wrongs.clone());
            if !result.matched_answers.contains_key(&local_q.id) {
                let label_text = unique_wrongs
                    .iter()
                    .map(|id| {
                        local_q
                            .options
                            .iter()
                            .find(|o| o.id == *id)
                            .map(|o| o.text.as_str())
                            .unwrap_or(id.as_str())
                    })
                    .collect::<Vec<_>>()
                    .join("、");
                result.reviews.push(Review {
                    question_id: local_q.id.clone(),
                    answers: vec![],
                    probabilities: Default::default(),
                    message: format!("题库已排除错误选项：{label_text}，请勿选择"),
                });
            }
        }
    }

    result
}

fn map_option_values(values: &[String], options: &[OptionItem]) -> Vec<String> {
    if options.is_empty() {
        return values.to_vec();
    }
    let mut mapped = Vec::new();
    for v in values {
        let v_trim = v.trim();
        // Exact option id match
        if let Some(opt) = options.iter().find(|o| o.id == v_trim || o.id.eq_ignore_ascii_case(v_trim)) {
            mapped.push(opt.id.clone());
            continue;
        }
        // Match by index letter ("A" -> 0, "B" -> 1, etc.)
        if v_trim.len() == 1 {
            let ch = v_trim.chars().next().unwrap().to_ascii_uppercase();
            if ch.is_ascii_alphabetic() {
                let idx = (ch as u8 - b'A') as usize;
                if idx < options.len() {
                    mapped.push(options[idx].id.clone());
                    continue;
                }
            }
        }
    }
    mapped
}

/// Build batch upload payload when quiz is confirmed passed.
pub(crate) fn build_passed_payload(
    cid: &str,
    course_name: Option<&str>,
    task_id: &str,
    task_title: &str,
    questions: &[Question],
    answers: &Answers,
) -> QaBatchUploadPayload {
    let upload_questions = questions
        .iter()
        .map(|q| {
            let correct = answers.get(&q.id).cloned();
            QaUploadQuestion {
                question_id: q.id.clone(),
                question_type: match q.kind {
                    Kind::Single => "single".into(),
                    Kind::Multiple => "multiple".into(),
                    Kind::Text => "text".into(),
                },
                stem: Some(q.text.clone()),
                options: if q.options.is_empty() {
                    None
                } else {
                    Some(
                        q.options
                            .iter()
                            .map(|o| QaUploadOption {
                                id: o.id.clone(),
                                text: o.text.clone(),
                            })
                            .collect(),
                    )
                },
                blanks: if q.blanks > 0 { Some(q.blanks) } else { None },
                correct_answers: correct,
                wrong_answers: None,
                embedding: None,
            }
        })
        .collect();

    QaBatchUploadPayload {
        tid: task_id.to_string(),
        cid: Some(cid.to_string()),
        task_name: Some(task_title.to_string()),
        course_name: course_name.map(str::to_string),
        questions: upload_questions,
    }
}

/// Build batch upload payload when quiz submission failed.
pub(crate) fn build_failed_payload(
    cid: &str,
    course_name: Option<&str>,
    task_id: &str,
    task_title: &str,
    questions: &[Question],
    answers: &Answers,
) -> QaBatchUploadPayload {
    let upload_questions = questions
        .iter()
        .map(|q| {
            let wrong = answers.get(&q.id).cloned();
            QaUploadQuestion {
                question_id: q.id.clone(),
                question_type: match q.kind {
                    Kind::Single => "single".into(),
                    Kind::Multiple => "multiple".into(),
                    Kind::Text => "text".into(),
                },
                stem: Some(q.text.clone()),
                options: if q.options.is_empty() {
                    None
                } else {
                    Some(
                        q.options
                            .iter()
                            .map(|o| QaUploadOption {
                                id: o.id.clone(),
                                text: o.text.clone(),
                            })
                            .collect(),
                    )
                },
                blanks: if q.blanks > 0 { Some(q.blanks) } else { None },
                correct_answers: None,
                wrong_answers: wrong,
                embedding: None,
            }
        })
        .collect();

    QaBatchUploadPayload {
        tid: task_id.to_string(),
        cid: Some(cid.to_string()),
        task_name: Some(task_title.to_string()),
        course_name: course_name.map(str::to_string),
        questions: upload_questions,
    }
}

// ─────────────────────────────────────────────────────────────
// 本地题目 ID 库管理与自动匹配上传 (Bank Matching & Sync)
// ─────────────────────────────────────────────────────────────

pub(crate) fn bank_key(tid: &str, question_id: &str) -> String {
    format!("{tid}:{question_id}")
}

pub(crate) fn load_local_bank(bank_path: &Path) -> BTreeMap<String, LocalBankQuestion> {
    crate::uooc::persistence::read::<BTreeMap<String, LocalBankQuestion>>(bank_path)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn save_local_bank(
    bank_path: &Path,
    bank: &BTreeMap<String, LocalBankQuestion>,
) -> Result<(), String> {
    crate::uooc::persistence::write(bank_path, bank)
}

/// 录入已通过并确认答案的测验试题到本地题库表，标记 uploaded 状态
pub(crate) fn record_passed_quiz(
    bank_path: &Path,
    cid: &str,
    task_id: &str,
    questions: &[Question],
    answers: &Answers,
) -> Result<usize, String> {
    let mut bank = load_local_bank(bank_path);
    let mut count = 0;
    let now = chrono::Utc::now().timestamp_millis();

    for q in questions {
        if let Some(ans) = answers.get(&q.id) {
            if !ans.is_empty() {
                let key = bank_key(task_id, &q.id);
                let already_uploaded = bank
                    .get(&key)
                    .map(|old| old.uploaded && old.correct_answers == *ans)
                    .unwrap_or(false);

                let record = LocalBankQuestion {
                    cid: cid.to_string(),
                    tid: task_id.to_string(),
                    question_id: q.id.clone(),
                    question_type: match q.kind {
                        Kind::Single => "single".into(),
                        Kind::Multiple => "multiple".into(),
                        Kind::Text => "text".into(),
                    },
                    stem: Some(q.text.clone()),
                    options: if q.options.is_empty() {
                        None
                    } else {
                        Some(
                            q.options
                                .iter()
                                .map(|o| QaUploadOption {
                                    id: o.id.clone(),
                                    text: o.text.clone(),
                                })
                                .collect(),
                        )
                    },
                    blanks: if q.blanks > 0 { Some(q.blanks) } else { None },
                    correct_answers: ans.clone(),
                    uploaded: already_uploaded,
                    updated_at_ms: now,
                };
                bank.insert(key, record);
                count += 1;
            }
        }
    }

    save_local_bank(bank_path, &bank)?;
    Ok(count)
}

/// 针对本地维护的确认答案题目列表，调用 POST /api/automation/match 进行服务端缺失确认，
/// 并自动将服务端缺失（或本地未上传）的题库打包上传至 /api/upload/batch
pub(crate) async fn sync_missing_to_server(
    bank_path: &Path,
    server_url: &str,
    api_key: Option<&str>,
    cid: Option<&str>,
    tid: Option<&str>,
) -> Result<usize, String> {
    let mut bank = load_local_bank(bank_path);
    if bank.is_empty() {
        return Ok(0);
    }

    let candidate_keys: Vec<String> = bank
        .iter()
        .filter(|(_, q)| {
            if let Some(target_cid) = cid {
                if q.cid != target_cid {
                    return false;
                }
            }
            if let Some(target_tid) = tid {
                if q.tid != target_tid {
                    return false;
                }
            }
            true
        })
        .map(|(k, _)| k.clone())
        .collect();

    if candidate_keys.is_empty() {
        return Ok(0);
    }

    let match_items: Vec<BankMatchItem> = candidate_keys
        .iter()
        .filter_map(|k| bank.get(k))
        .map(|q| BankMatchItem {
            tid: q.tid.clone(),
            question_id: q.question_id.clone(),
        })
        .collect();

    let payload = BankMatchPayload {
        cid: cid.map(str::to_string),
        tid: tid.map(str::to_string),
        items: Some(match_items),
        local_question_ids: None,
    };

    let base = server_url.trim_end_matches('/');
    let url = format!("{base}/api/automation/match");
    let client = http_client()?;
    let mut req = client.post(&url).json(&payload);
    if let Some(key) = api_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            req = req.header("x-api-key", trimmed);
        }
    }

    let resp = req.send().await.map_err(|e| format!("匹配题库请求失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("匹配题库响应失败，状态码: {}", resp.status()));
    }

    let match_res: BankMatchResponse = resp
        .json()
        .await
        .map_err(|e| format!("解析匹配题库响应失败: {e}"))?;

    // 汇总服务端缺失以及本地尚未成功标记上传的题目 key
    let mut to_upload_keys = std::collections::HashSet::new();
    for missing in &match_res.missing_on_server {
        let missing_tid = missing.tid.as_deref().or(tid);
        if let Some(t) = missing_tid {
            to_upload_keys.insert(bank_key(t, &missing.question_id));
        } else {
            for k in &candidate_keys {
                if let Some(q) = bank.get(k) {
                    if q.question_id == missing.question_id {
                        to_upload_keys.insert(k.clone());
                    }
                }
            }
        }
    }

    // 同时包含本地已记录但 uploaded == false 的题目
    for k in &candidate_keys {
        if let Some(q) = bank.get(k) {
            if !q.uploaded {
                to_upload_keys.insert(k.clone());
            }
        }
    }

    if to_upload_keys.is_empty() {
        return Ok(0);
    }

    // 按 tid 分组打包批量上传
    let mut by_tid: BTreeMap<String, Vec<LocalBankQuestion>> = BTreeMap::new();
    for k in &to_upload_keys {
        if let Some(q) = bank.get(k) {
            by_tid.entry(q.tid.clone()).or_default().push(q.clone());
        }
    }

    let mut uploaded_count = 0;
    for (t, qs) in by_tid {
        let first_cid = qs.first().map(|q| q.cid.clone());
        let upload_payload = QaBatchUploadPayload {
            tid: t,
            cid: first_cid,
            task_name: None,
            course_name: None,
            questions: qs
                .iter()
                .map(|q| QaUploadQuestion {
                    question_id: q.question_id.clone(),
                    question_type: q.question_type.clone(),
                    stem: q.stem.clone(),
                    options: q.options.clone(),
                    blanks: q.blanks,
                    correct_answers: Some(q.correct_answers.clone()),
                    wrong_answers: None,
                    embedding: None,
                })
                .collect(),
        };

        if let Ok(res) = upload_batch(server_url, api_key, &upload_payload).await {
            if res.ok {
                for q in qs {
                    let k = bank_key(&q.tid, &q.question_id);
                    if let Some(item) = bank.get_mut(&k) {
                        item.uploaded = true;
                        uploaded_count += 1;
                    }
                }
            }
        }
    }

    let _ = save_local_bank(bank_path, &bank);
    Ok(uploaded_count)
}

// ─────────────────────────────────────────────────────────────
// 本地题库向量缓存与向量检索 (Vector Cache & Search)
// ─────────────────────────────────────────────────────────────

pub(crate) fn load_vector_cache(cache_path: &Path) -> Vec<CachedVectorQuestion> {
    crate::uooc::persistence::read::<Vec<CachedVectorQuestion>>(cache_path)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn save_vector_cache(
    cache_path: &Path,
    vectors: &[CachedVectorQuestion],
) -> Result<(), String> {
    crate::uooc::persistence::write(cache_path, &vectors)
}

pub(crate) fn get_vector_status(cache_path: &Path) -> EmbeddingModelStatus {
    let list = load_vector_cache(cache_path);
    let count = list.len();
    EmbeddingModelStatus {
        status: if count > 0 { "ready".into() } else { "empty".into() },
        cached_count: count,
        total_dimension: EMBEDDING_DIMENSIONS,
        updated_at_ms: chrono::Utc::now().timestamp_millis(),
        message: if count > 0 {
            format!("本地已缓存 {count} 条题目向量，支持高速离线与远程向量检索")
        } else {
            "尚未下载题库向量，点击下方按钮开始下载".into()
        },
    }
}

/// 从服务端批量拉取题目并缓存 1536 维向量
pub(crate) async fn download_qa_vectors(
    cache_path: &Path,
    server_url: &str,
    api_key: Option<&str>,
    cids: &[String],
) -> Result<EmbeddingModelStatus, String> {
    let mut cached = load_vector_cache(cache_path);
    let mut seen_ids = std::collections::HashSet::new();
    for item in &cached {
        seen_ids.insert(format!("{}:{}", item.tid, item.question_id));
    }

    let base = server_url.trim_end_matches('/');
    let client = http_client()?;

    // 1. 如果提供了课程列表，优先按课程抓取题组
    let mut tids_to_fetch = std::collections::HashSet::new();
    for cid in cids {
        let url = format!("{base}/api/courses/{cid}");
        let mut req = client.get(&url);
        if let Some(key) = api_key {
            req = req.header("x-api-key", key);
        }
        if let Ok(resp) = req.send().await {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if let Some(groups) = data["taskGroups"].as_array() {
                    for g in groups {
                        if let Some(tid) = g["tid"].as_str() {
                            tids_to_fetch.insert((cid.clone(), tid.to_string()));
                        }
                    }
                }
            }
        }
    }

    // 2. 同时从 /api/automation/tasks 发现所有缺失或现存的题组
    let auto_url = format!("{base}/api/automation/tasks?type=all&limit=100");
    let mut auto_req = client.get(&auto_url);
    if let Some(key) = api_key {
        auto_req = auto_req.header("x-api-key", key);
    }
    if let Ok(resp) = auto_req.send().await {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            if let Some(tasks) = data["tasks"].as_array() {
                for t in tasks {
                    let cid = t["cid"].as_str().unwrap_or("").to_string();
                    if let Some(tid) = t["tid"].as_str() {
                        tids_to_fetch.insert((cid, tid.to_string()));
                    }
                }
            }
        }
    }

    // 3. 逐个题组拉取题目并生成/记录向量
    for (cid, tid) in tids_to_fetch {
        if let Ok(qs) = query_task_answers(server_url, api_key, &tid, if cid.is_empty() { None } else { Some(&cid) }).await {
            for q in qs {
                let Some(ref qid) = q.question_id else { continue };
                let key = format!("{tid}:{qid}");
                if seen_ids.insert(key) {
                    let stem_text = q.stem.clone().unwrap_or_default();
                    let embedding = match q.stem_embedding {
                        Some(ref v) if v.len() == EMBEDDING_DIMENSIONS => v.clone(),
                        _ => compute_local_embedding(&stem_text),
                    };

                    let answers = q
                        .answers
                        .iter()
                        .filter(|a| a.is_correct)
                        .flat_map(|a| a.values.clone())
                        .collect();

                    cached.push(CachedVectorQuestion {
                        id: q.id,
                        tid: tid.clone(),
                        cid: if cid.is_empty() { None } else { Some(cid.clone()) },
                        question_id: qid.clone(),
                        question_type: q.question_type.unwrap_or_else(|| "single".into()),
                        stem: stem_text,
                        embedding,
                        options: q.options.map(|opts| {
                            opts.into_iter()
                                .map(|o| QaUploadOption { id: o.id, text: o.text })
                                .collect()
                        }),
                        answers,
                    });
                }
            }
        }
    }

    save_vector_cache(cache_path, &cached)?;
    Ok(get_vector_status(cache_path))
}

/// 本地完成 1536 维向量计算后搜索题目（结合本地向量缓存余弦相似度与服务端远程搜索）
pub(crate) async fn search_questions_with_local_embedding(
    cache_path: &Path,
    server_url: &str,
    api_key: Option<&str>,
    query: &str,
    limit: usize,
) -> Result<Vec<QaSearchResultItem>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    // 1. 本地计算 1536 维特征向量
    let local_vector = compute_local_embedding(trimmed);

    let mut results: Vec<QaSearchResultItem> = Vec::new();
    let mut seen_qids = std::collections::HashSet::new();

    // 2. 本地已下载缓存的向量库检索 (余弦相似度计算)
    let cached = load_vector_cache(cache_path);
    for item in &cached {
        let sim = cosine_similarity(&local_vector, &item.embedding);
        if sim > 0.3 || normalize_stem(&item.stem).contains(&normalize_stem(trimmed)) {
            seen_qids.insert(item.question_id.clone());
            results.push(QaSearchResultItem {
                question_id: item.question_id.clone(),
                tid: Some(item.tid.clone()),
                cid: item.cid.clone(),
                stem: item.stem.clone(),
                question_type: item.question_type.clone(),
                similarity: sim,
                answers: item.answers.clone(),
                source: "本地题库向量检索".into(),
            });
        }
    }

    // 3. 远程调用 POST /api/questions/search 加速搜索（传递本地向量，服务端免计算直接余弦排序）
    let base = server_url.trim_end_matches('/');
    let url = format!("{base}/api/questions/search");
    let client = http_client()?;
    let search_payload = serde_json::json!({
        "q": trimmed,
        "embedding": local_vector,
        "limit": limit.min(20),
    });

    let mut req = client.post(&url).json(&search_payload);
    if let Some(key) = api_key {
        let trimmed_key = key.trim();
        if !trimmed_key.is_empty() {
            req = req.header("x-api-key", trimmed_key);
        }
    }

    if let Ok(resp) = req.send().await {
        if resp.status().is_success() {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if let Some(list) = data["results"].as_array() {
                    for row in list {
                        let qid = row["questionId"]
                            .as_str()
                            .or_else(|| row["question_id"].as_str())
                            .or_else(|| row["id"].as_i64().map(|id| id.to_string().leak() as &str))
                            .unwrap_or("")
                            .to_string();

                        if !qid.is_empty() && !seen_qids.contains(&qid) {
                            seen_qids.insert(qid.clone());
                            let sim = row["similarity"].as_f64().unwrap_or(0.85);
                            let stem = row["stem"].as_str().unwrap_or("").to_string();
                            let q_type = row["type"].as_str().unwrap_or("single").to_string();
                            results.push(QaSearchResultItem {
                                question_id: qid,
                                tid: row["tid"].as_str().map(str::to_string),
                                cid: row["cid"].as_str().map(str::to_string),
                                stem,
                                question_type: q_type,
                                similarity: sim,
                                answers: vec![],
                                source: "服务端向量加速检索".into(),
                            });
                        }
                    }
                }
            }
        }
    }

    // 按相似度倒序排序
    results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_normalization_and_hashing_matches_specification() {
        let raw = "<p> 关于微积分基本定理，下列说法正确的是？ &nbsp; </p>";
        let normalized = normalize_stem(raw);
        assert_eq!(normalized, "关于微积分基本定理，下列说法正确的是？");
        let hash = hash_stem(raw);
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, hash_stem("关于微积分基本定理，下列说法正确的是？"));
    }

    #[test]
    fn local_embedding_vector_has_correct_dimension_and_unit_norm() {
        let text1 = "高等数学微积分定理分析";
        let text2 = "高等数学微积分定理证明";
        let text3 = "近代史纲要重大历史事件总结";

        let vec1 = compute_local_embedding(text1);
        let vec2 = compute_local_embedding(text2);
        let vec3 = compute_local_embedding(text3);

        assert_eq!(vec1.len(), EMBEDDING_DIMENSIONS);
        assert_eq!(vec2.len(), EMBEDDING_DIMENSIONS);
        assert_eq!(vec3.len(), EMBEDDING_DIMENSIONS);

        let norm1: f32 = vec1.iter().map(|v| v * v).sum();
        assert!((norm1.sqrt() - 1.0).abs() < 1e-4);

        let sim12 = cosine_similarity(&vec1, &vec2);
        let sim13 = cosine_similarity(&vec1, &vec3);

        // 相似主题的文本余弦相似度应当高于不同主题文本
        assert!(sim12 > sim13);
    }

    #[test]
    fn match_answers_extracts_correct_and_wrong_answers() {
        let qa_questions = vec![
            QaQuestion {
                id: Some(1),
                tid: Some("t1".into()),
                question_id: Some("q1".into()),
                question_type: Some("single".into()),
                stem: Some("1 + 1 = ?".into()),
                stem_hash: Some(hash_stem("1 + 1 = ?")),
                options: Some(vec![
                    QaOption { id: "opt-a".into(), text: "2".into() },
                    QaOption { id: "opt-b".into(), text: "3".into() },
                ]),
                answers: vec![
                    QaAnswer {
                        id: Some(10),
                        values: vec!["opt-a".into()],
                        is_correct: true,
                        confidence: Some(1.0),
                    },
                ],
                stem_embedding: None,
            },
            QaQuestion {
                id: Some(2),
                tid: Some("t1".into()),
                question_id: Some("q2".into()),
                question_type: Some("single".into()),
                stem: Some("<p>地球是平的吗？</p>".into()),
                stem_hash: Some(hash_stem("地球是平的吗？")),
                options: None,
                answers: vec![
                    QaAnswer {
                        id: Some(11),
                        values: vec!["A".into()],
                        is_correct: false,
                        confidence: Some(0.5),
                    },
                ],
                stem_embedding: None,
            },
        ];

        let local_questions = vec![
            Question {
                id: "q1".into(),
                text: "1 + 1 = ?".into(),
                kind: Kind::Single,
                options: vec![
                    OptionItem { id: "opt-a".into(), text: "2".into() },
                    OptionItem { id: "opt-b".into(), text: "3".into() },
                ],
                blanks: 0,
                images: vec![],
            },
            Question {
                id: "q2".into(),
                text: "地球是平的吗？".into(),
                kind: Kind::Single,
                options: vec![
                    OptionItem { id: "A".into(), text: "是".into() },
                    OptionItem { id: "B".into(), text: "否".into() },
                ],
                blanks: 0,
                images: vec![],
            },
        ];

        let result = match_answers(&qa_questions, &local_questions);
        assert_eq!(result.matched_answers.get("q1"), Some(&vec!["opt-a".into()]));
        assert_eq!(result.wrong_options.get("q2"), Some(&vec!["A".into()]));
        assert!(result.reviews.iter().any(|r| r.question_id == "q1" && r.message.contains("命中标准答案")));
        assert!(result.reviews.iter().any(|r| r.question_id == "q2" && r.message.contains("排除错误选项")));
    }

    #[tokio::test]
    async fn local_bank_and_sync_missing_to_server() {
        use crate::uooc::tests::Server;
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let bank_path = dir.path().join("qa-bank.json");

        let mut answers = Answers::new();
        answers.insert("q1".into(), vec!["A".into()]);
        let questions = vec![Question {
            id: "q1".into(),
            text: "1 + 1 = ?".into(),
            kind: Kind::Single,
            options: vec![
                OptionItem { id: "A".into(), text: "2".into() },
                OptionItem { id: "B".into(), text: "3".into() },
            ],
            blanks: 0,
            images: vec![],
        }];

        // 1. Record passed quiz into local bank
        let recorded = record_passed_quiz(&bank_path, "200", "100", &questions, &answers).unwrap();
        assert_eq!(recorded, 1);
        let bank = load_local_bank(&bank_path);
        assert_eq!(bank.len(), 1);
        assert_eq!(bank.get("100:q1").unwrap().uploaded, false);

        // 2. Mock server for match and batch upload
        let match_calls = Arc::new(AtomicUsize::new(0));
        let match_calls_clone = match_calls.clone();
        let upload_calls = Arc::new(AtomicUsize::new(0));
        let upload_calls_clone = upload_calls.clone();

        let server = Server::new(move |request| {
            if request.path == "/api/automation/match" && request.method == "POST" {
                match_calls_clone.fetch_add(1, Ordering::SeqCst);
                return json!({
                    "ok": true,
                    "missingOnServer": [
                        { "cid": "200", "tid": "100", "questionId": "q1" }
                    ]
                });
            }
            if request.path == "/api/upload/batch" && request.method == "POST" {
                upload_calls_clone.fetch_add(1, Ordering::SeqCst);
                let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
                assert_eq!(body["tid"], "100");
                assert_eq!(body["questions"][0]["question_id"], "q1");
                return json!({
                    "ok": true,
                    "results": [{ "question_id": "q1", "action": "added_correct" }]
                });
            }
            json!({ "error": "not found" })
        });

        let uploaded_count = sync_missing_to_server(&bank_path, &server.base, Some("test_key"), Some("200"), Some("100"))
            .await
            .unwrap();
        assert_eq!(uploaded_count, 1);
        assert_eq!(match_calls.load(Ordering::SeqCst), 1);
        assert_eq!(upload_calls.load(Ordering::SeqCst), 1);

        // Check local bank marked as uploaded
        let updated_bank = load_local_bank(&bank_path);
        assert_eq!(updated_bank.get("100:q1").unwrap().uploaded, true);
    }
}
