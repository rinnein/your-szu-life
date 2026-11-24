mod parse;
mod prefs;
mod prompt;
mod solver;
mod tools;

use super::quiz::model::{AiSessionEntry, Answers, Question, Review};
use super::Service;
use std::collections::{BTreeMap, HashSet};
use std::sync::{LazyLock, Mutex};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub use prefs::{Input as PrefsInput, Prefs, View as PrefsView};
pub use prompt::ReferenceExample;

use prompt::SolverOptions;
use szu_ai::ProviderConfig;

/// 进程内记录「当前模型不支持图片输入」，避免每个批次都先失败再降级。
static VISION_UNSUPPORTED_MODELS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

fn vision_key(provider: &ProviderConfig, model: &str) -> String {
    format!("{}\u{1}{}", provider.id, model)
}

fn vision_is_disabled(provider: &ProviderConfig, model: &str) -> bool {
    VISION_UNSUPPORTED_MODELS
        .lock()
        .ok()
        .is_some_and(|models| models.contains(&vision_key(provider, model)))
}

fn mark_vision_disabled(provider: &ProviderConfig, model: &str) {
    if let Ok(mut models) = VISION_UNSUPPORTED_MODELS.lock() {
        models.insert(vision_key(provider, model));
    }
}

fn fail(message: String) -> (Answers, Vec<Review>, Vec<AiSessionEntry>, Option<String>) {
    (Answers::new(), Vec::new(), Vec::new(), Some(message))
}

pub async fn generate(
    service: &Service,
    questions: &[Question],
    wrong_options_map: &BTreeMap<String, Vec<String>>,
    cancel: &CancellationToken,
    progress_callback: Option<&(dyn Fn(&AiSessionEntry) + Send + Sync)>,
) -> (Answers, Vec<Review>, Vec<AiSessionEntry>, Option<String>) {
    let prefs = service.ai_prefs.lock().unwrap().clone().normalized();
    let global = match szu_ai::store::load(service.credentials.as_ref()) {
        Ok(config) => config,
        Err(error) => return fail(error),
    };
    let Some((provider, model)) = global
        .selected()
        .map(|(provider, model)| (provider.clone(), model))
    else {
        return fail("请先在设置 → AI 配置中添加模型提供商并选择默认模型".into());
    };
    if prefs.web_search && provider.vendor.needs_api_key() && provider.api_key.is_empty() {
        return fail("默认 AI 提供商缺少 API Key，请到设置 → AI 配置补全".into());
    }

    let batch_size = prefs.batch_size.clamp(1, 20);
    let mut all_answers = Answers::new();
    let mut all_reviews = Vec::new();
    let mut all_sessions = Vec::new();
    let in_memory_rag = if let Ok(qa_db) = service.get_qa_db().await {
        qa_db.build_in_memory_rag_store(None, None).await.ok()
    } else {
        None
    };

    let chunks: Vec<&[Question]> = questions.chunks(batch_size).collect();
    for (batch_idx, chunk) in chunks.iter().enumerate() {
        let permit = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                return (all_answers, all_reviews, all_sessions, Some("已取消".into()));
            }
            permit = service.ai_permits.acquire() => match permit {
                Ok(permit) => permit,
                Err(_) => {
                    return (all_answers, all_reviews, all_sessions, Some("AI 服务不可用".into()));
                }
            }
        };

        let mut session_entry = AiSessionEntry {
            batch_index: batch_idx + 1,
            question_ids: chunk.iter().map(|question| question.id.clone()).collect(),
            question_titles: chunk
                .iter()
                .map(|question| question.text.chars().take(40).collect())
                .collect(),
            model: model.clone(),
            prompt: format!(
                "正在准备批次 {}（共 {} 题）的提示词并请求模型...",
                batch_idx + 1,
                chunk.len()
            ),
            response: String::new(),
            answers: BTreeMap::new(),
            status: "pending".into(),
            error: None,
        };
        if let Some(callback) = progress_callback {
            callback(&session_entry);
        }

        let options = SolverOptions {
            include_reasoning: prefs.include_reasoning,
            web_search: prefs.web_search,
            reasoning_effort: prefs.reasoning_effort.clone(),
        };
        let include_images = !vision_is_disabled(&provider, &model);
        let built = prompt::build_batch_payload(
            chunk,
            wrong_options_map,
            in_memory_rag.as_ref(),
            options.clone(),
            include_images,
        );
        session_entry.prompt = built.text.clone();
        if built.rag_count > 0 {
            session_entry.response = format!(
                "[RAG 检索] 本地题库已匹配 {} 道题目的已验证考点作为参考上下文\n\n",
                built.rag_count
            );
        }
        if let Some(callback) = progress_callback {
            callback(&session_entry);
        }

        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<String>();
        let mut solving = Box::pin(solver::solve(
            solver::Settings {
                provider: provider.clone(),
                model: model.clone(),
            },
            built,
            cancel,
            Some(progress_tx),
        ));
        let mut live_log = String::new();
        let result = loop {
            tokio::select! {
                result = &mut solving => break result,
                Some(text) = progress_rx.recv() => {
                    live_log.push_str(&text);
                    if let Some(callback) = progress_callback {
                        let mut entry = session_entry.clone();
                        entry.response = live_log.clone();
                        callback(&entry);
                    }
                }
            }
        };
        drop(permit);

        match result {
            Ok(outcome) => {
                if outcome.images_omitted {
                    mark_vision_disabled(&provider, &model);
                }
                let parsed = match parse::parse_answers(&outcome.raw, chunk) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        session_entry.status = "error".into();
                        session_entry.error = Some(error.clone());
                        if let Some(callback) = progress_callback {
                            callback(&session_entry);
                        }
                        all_sessions.push(session_entry);
                        return (all_answers, all_reviews, all_sessions, Some(error));
                    }
                };
                session_entry.status = "completed".into();
                session_entry.answers = parsed.answers.clone();
                session_entry.response = if outcome.tool_log.is_empty() {
                    outcome.raw.clone()
                } else {
                    format!("{}\n{}", outcome.tool_log.trim_end(), outcome.raw)
                };
                let images_omitted = outcome.images_omitted
                    || (!include_images && chunk.iter().any(|q| !q.images.is_empty()));

                for question in *chunk {
                    let selected = parsed
                        .answers
                        .get(&question.id)
                        .cloned()
                        .unwrap_or_default();
                    if images_omitted && !question.images.is_empty() {
                        let only_images = prompt::only_images(question);
                        all_reviews.push(Review {
                            question_id: question.id.clone(),
                            answers: selected.clone(),
                            message: if only_images {
                                format!(
                                    "当前模型不支持图片输入，题目仅含图片，无法可靠作答（共 {} 张图片）",
                                    question.images.len()
                                )
                            } else {
                                format!(
                                    "当前模型不支持图片输入，已按纯文本作答（本题含 {} 张图片）",
                                    question.images.len()
                                )
                            },
                            ..Default::default()
                        });
                    }
                    if prefs.web_search && outcome.tools_disabled {
                        all_reviews.push(Review {
                            question_id: question.id.clone(),
                            answers: selected.clone(),
                            message: "当前模型不支持工具调用，已关闭联网搜索".into(),
                            ..Default::default()
                        });
                    }
                    let reason = parsed
                        .reasons
                        .get(&question.id)
                        .filter(|reason| !reason.trim().is_empty())
                        .map(|reason| format!("\n理由/解析：{}", reason.trim()))
                        .unwrap_or_default();
                    all_reviews.push(Review {
                        question_id: question.id.clone(),
                        answers: selected.clone(),
                        probabilities: Default::default(),
                        message: format!("AI 模型 ({model}) 判定推荐答案{reason}"),
                    });
                    all_answers.insert(question.id.clone(), selected);
                }

                if let Some(callback) = progress_callback {
                    callback(&session_entry);
                }
                all_sessions.push(session_entry);
            }
            Err(error) => {
                session_entry.status = "error".into();
                session_entry.error = Some(error.clone());
                if let Some(callback) = progress_callback {
                    callback(&session_entry);
                }
                all_sessions.push(session_entry);
                return (all_answers, all_reviews, all_sessions, Some(error));
            }
        }
    }

    (all_answers, all_reviews, all_sessions, None)
}

pub async fn generate_subjective_answer(
    service: &Service,
    stem: &str,
    context: Option<&str>,
    reference_examples: &[ReferenceExample],
    images: &[String],
    cancel: &CancellationToken,
) -> Result<String, String> {
    let prefs = service.ai_prefs.lock().unwrap().clone().normalized();
    let global = szu_ai::store::load(service.credentials.as_ref())?;
    let Some((provider, model)) = global
        .selected()
        .map(|(provider, model)| (provider.clone(), model))
    else {
        return Err("请先在设置 → AI 配置中添加模型提供商并选择默认模型".into());
    };
    let include_images = !vision_is_disabled(&provider, &model);
    let options = SolverOptions {
        include_reasoning: prefs.include_reasoning,
        web_search: false,
        reasoning_effort: prefs.reasoning_effort.clone(),
    };
    let built = prompt::build_subjective_payload(
        stem,
        context,
        reference_examples,
        images,
        options.clone(),
        include_images,
    );
    let outcome = solver::solve(
        solver::Settings {
            provider: provider.clone(),
            model: model.clone(),
        },
        built,
        cancel,
        None,
    )
    .await?;
    if outcome.images_omitted {
        mark_vision_disabled(&provider, &model);
    }
    if outcome.raw.trim().is_empty() {
        return Err("AI 返回了空答案".into());
    }
    Ok(outcome.raw.trim().to_string())
}
