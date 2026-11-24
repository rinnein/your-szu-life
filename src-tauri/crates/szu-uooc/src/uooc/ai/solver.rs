use super::prompt::{BuiltPrompt, SolverMode, SolverPayload};
use super::tools::{self, ToolEvent};
use agent_client_protocol::{
    on_receive_notification, on_receive_request,
    schema::ProtocolVersion,
    util::MatchDispatch,
    v1::schema::{
        AgentCapabilities, CancelNotification, ContentBlock, ContentChunk, ImageContent,
        InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse,
        PromptCapabilities, PromptRequest, PromptResponse, SessionId, SessionNotification,
        SessionUpdate, StopReason, TextContent,
    },
    v1::{ConnectionTo, Responder, SessionMessage, SessionSetup},
    Agent, Client, Error as AcpError, ErrorCode,
};
use rig_agent::AgentBuilder;
use rig_core::completion::message::{ImageMediaType, Message, UserContent};
use rig_core::completion::{Effort, GenerationOptions};
use rig_core::message::MimeType;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use szu_ai::ProviderConfig;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct Settings {
    pub provider: ProviderConfig,
    pub model: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    #[serde(default)]
    pub raw: String,
    #[serde(default)]
    pub tool_log: String,
    #[serde(default)]
    pub images_omitted: bool,
    #[serde(default)]
    pub tools_disabled: bool,
    #[serde(default)]
    pub error: Option<String>,
}

struct AgentState {
    settings: Settings,
    sessions: Mutex<HashMap<SessionId, CancellationToken>>,
}

#[derive(Clone)]
struct AgentImage {
    data: String,
    mime_type: String,
}

/// 进程内 ACP 往返：Client 发 prompt，Agent 用 rig Agent 跑模型与工具，
/// 工具进度通过 session/update 回到 Client，最终结构化结果放在 PromptResponse._meta。
pub async fn solve(
    settings: Settings,
    built: BuiltPrompt,
    cancel: &CancellationToken,
    progress: Option<mpsc::UnboundedSender<String>>,
) -> Result<Outcome, String> {
    let state = Arc::new(AgentState {
        settings,
        sessions: Mutex::new(HashMap::new()),
    });
    let (client_transport, agent_transport) = agent_client_protocol::Channel::duplex();

    let state_init = state.clone();
    let state_new = state.clone();
    let state_prompt = state.clone();
    let state_cancel = state.clone();
    let agent = Agent
        .v1()
        .name("your-szu-life-uooc-solver")
        .on_receive_request(
            async move |req: InitializeRequest, responder: Responder<InitializeResponse>, _cx| {
                let _ = &state_init;
                responder.respond(
                    InitializeResponse::new(req.protocol_version).agent_capabilities(
                        AgentCapabilities::new()
                            .prompt_capabilities(PromptCapabilities::new().image(true)),
                    ),
                )
            },
            on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: NewSessionRequest, responder: Responder<NewSessionResponse>, _cx| {
                let id = SessionId::new(uuid::Uuid::new_v4().to_string());
                state_new
                    .sessions
                    .lock()
                    .unwrap()
                    .insert(id.clone(), CancellationToken::new());
                responder.respond(NewSessionResponse::new(id))
            },
            on_receive_request!(),
        )
        .on_receive_request(
            async move |req: PromptRequest, responder: Responder<PromptResponse>, connection| {
                let outcome = run_prompt(&state_prompt, &req, &connection).await;
                state_prompt
                    .sessions
                    .lock()
                    .unwrap()
                    .remove(&req.session_id);
                match outcome {
                    Ok(outcome) => {
                        let meta = to_meta(&outcome);
                        responder.respond(PromptResponse::new(StopReason::EndTurn).meta(meta))
                    }
                    Err(error) => {
                        let mut meta = serde_json::Map::new();
                        meta.insert("error".into(), Value::String(error));
                        responder.respond(PromptResponse::new(StopReason::Refusal).meta(meta))
                    }
                }
            },
            on_receive_request!(),
        )
        .on_receive_notification(
            async move |notification: CancelNotification, _cx| {
                if let Some(token) = state_cancel
                    .sessions
                    .lock()
                    .unwrap()
                    .get(&notification.session_id)
                {
                    token.cancel();
                }
                Ok(())
            },
            on_receive_notification!(),
        );

    let agent_task = tokio::spawn(async move { agent.connect_to(agent_transport).await });
    let result = run_client(client_transport, built, cancel, progress).await;
    agent_task.abort();
    result
}

async fn run_client(
    transport: agent_client_protocol::Channel,
    built: BuiltPrompt,
    cancel: &CancellationToken,
    progress: Option<mpsc::UnboundedSender<String>>,
) -> Result<Outcome, String> {
    let mut blocks = vec![ContentBlock::Text(TextContent::new(built.text))];
    for image in built.images {
        blocks.push(ContentBlock::Image(ImageContent::new(
            image.data_base64,
            image.mime_type,
        )));
    }
    let cancel = cancel.clone();
    let outcome = Client
        .v1()
        .name("your-szu-life-uooc-client")
        .connect_with(
            transport,
            async move |cx: ConnectionTo<Agent>| {
                cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await
                    .map_err(|error| acp_error(format!("ACP 初始化失败: {error}")))?;
                cx.build_session_cwd()
                    .map_err(|error| acp_error(format!("ACP 会话创建失败: {error}")))?
                    .block_task()
                    .run_until(async move |SessionSetup { mut session, .. }| {
                        let session_id = session.session_id().clone();
                        session
                            .send_prompt_blocks(blocks)
                            .map_err(|error| acp_error(format!("ACP 提示词发送失败: {error}")))?;
                        let mut cancel_sent = false;
                        loop {
                            tokio::select! {
                                biased;
                                _ = cancel.cancelled(), if !cancel_sent => {
                                    cancel_sent = true;
                                    session
                                        .connection()
                                        .send_notification_to(
                                            Agent,
                                            CancelNotification::new(session_id.clone()),
                                        )
                                        .map_err(|error| acp_error(format!("ACP 取消失败: {error}")))?;
                                }
                                update = session.read_update() => {
                                    match update.map_err(|error| acp_error(format!("ACP 更新读取失败: {error}")))? {
                                        SessionMessage::Message(dispatch) => {
                                            let progress = progress.clone();
                                            MatchDispatch::new(dispatch.into_raw())
                                                .if_notification(async move |notification: SessionNotification| {
                                                    if let SessionUpdate::AgentMessageChunk(chunk) = notification.update {
                                                        if let ContentBlock::Text(text) = chunk.content {
                                                            if let Some(sender) = &progress {
                                                                let _ = sender.send(text.text);
                                                            }
                                                        }
                                                    }
                                                    Ok(())
                                                })
                                                .await
                                                .otherwise_ignore()
                                                .map_err(|error| acp_error(format!("ACP 通知处理失败: {error}")))?;
                                        }
                                        SessionMessage::PromptResponse(response) => {
                                            return outcome_from_response(
                                                response.map_err(|error| acp_error(format!("ACP 答题失败: {error}")))?,
                                            )
                                            .map_err(acp_error);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    })
                    .await
            },
        )
        .await
        .map_err(|error| format!("ACP 连接失败: {error}"))?;
    Ok(outcome)
}

fn acp_error(message: impl Into<String>) -> AcpError {
    AcpError::new(ErrorCode::INTERNAL_ERROR, message)
}

fn parse_reasoning_effort(raw: &str) -> Option<Effort> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "low" => Some(Effort::Low),
        "medium" => Some(Effort::Medium),
        "high" => Some(Effort::High),
        _ => None,
    }
}

fn outcome_from_response(response: PromptResponse) -> Result<Outcome, String> {
    if response.stop_reason == StopReason::Cancelled {
        return Err("已取消".into());
    }
    let outcome = response
        .meta
        .and_then(|meta| serde_json::from_value::<Outcome>(Value::Object(meta)).ok())
        .ok_or_else(|| "ACP 答题子部件未返回结果".to_string())?;
    if let Some(error) = &outcome.error {
        return Err(error.clone());
    }
    if outcome.raw.trim().is_empty() {
        return Err("AI 返回了空答案".into());
    }
    Ok(outcome)
}

async fn run_prompt(
    state: &AgentState,
    request: &PromptRequest,
    connection: &ConnectionTo<Client>,
) -> Result<Outcome, String> {
    let (payload_text, images) = parse_prompt(&request.prompt)?;
    let payload: SolverPayload =
        serde_json::from_str(&payload_text).map_err(|_| "ACP 提示词格式无效")?;
    let token = state
        .sessions
        .lock()
        .unwrap()
        .get(&request.session_id)
        .cloned()
        .unwrap_or_default();
    let system = super::prompt::system_prompt(payload.mode, &payload.options);
    let use_tools = payload.options.web_search && payload.mode == SolverMode::SolveBatch;
    let has_images = !images.is_empty();

    let mut message = build_message(&payload, &images, true);
    let mut images_omitted = false;
    let mut tools_disabled = false;

    let first = run_once(
        state,
        system.clone(),
        message.clone(),
        use_tools,
        &payload.options.reasoning_effort,
        &request.session_id,
        &token,
        connection,
    )
    .await;
    let (raw, tool_log) = match first {
        Ok(result) => result,
        Err(error) => {
            if token.is_cancelled() || error == "已取消" {
                return Err("已取消".into());
            }
            let vision_failure = has_images && tools::looks_like_vision_unsupported(&error);
            let tool_failure = use_tools && tools::looks_like_tool_unsupported(&error);
            if !vision_failure && !tool_failure {
                return Err(error);
            }
            if vision_failure {
                images_omitted = true;
                message = build_message(&payload, &images, false);
            }
            if tool_failure {
                tools_disabled = true;
            }
            run_once(
                state,
                system,
                message,
                use_tools && !tool_failure,
                &payload.options.reasoning_effort,
                &request.session_id,
                &token,
                connection,
            )
            .await?
        }
    };

    Ok(Outcome {
        raw,
        tool_log,
        images_omitted,
        tools_disabled,
        error: None,
    })
}

async fn run_once(
    state: &AgentState,
    system: String,
    message: Message,
    use_tools: bool,
    reasoning_effort: &str,
    session_id: &SessionId,
    token: &CancellationToken,
    connection: &ConnectionTo<Client>,
) -> Result<(String, String), String> {
    let model = szu_ai::model::build_provider_async(&state.settings.provider, &state.settings.model)
        .await?;
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<ToolEvent>();
    let mut builder = AgentBuilder::new(model)
        .preamble(system)
        .default_max_turns(6);
    if let Some(effort) = parse_reasoning_effort(reasoning_effort) {
        builder = builder.options(GenerationOptions::new().reasoning(effort));
    }
    if use_tools {
        builder = builder
            .dynamic_tool(tools::web_search_tool(progress_tx.clone()))
            .dynamic_tool(tools::web_fetch_tool(progress_tx.clone()));
    }
    let agent = builder.build();
    let run = agent.prompt(message).run();
    tokio::pin!(run);

    let mut tool_log = String::new();
    let response = loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => return Err("已取消".into()),
            result = &mut run => break result.map_err(|error| format!("AI 请求失败: {error}"))?,
            Some(event) = progress_rx.recv() => {
                tool_log.push_str(&event.text);
                connection
                    .send_notification(SessionNotification::new(
                        session_id.clone(),
                        SessionUpdate::AgentMessageChunk(ContentChunk::new(event.text.into())),
                    ))
                    .map_err(|error| format!("ACP 进度发送失败: {error}"))?;
            }
        }
    };
    while let Ok(event) = progress_rx.try_recv() {
        tool_log.push_str(&event.text);
        let _ = connection.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::AgentMessageChunk(ContentChunk::new(event.text.into())),
        ));
    }
    Ok((response.output(), tool_log))
}

fn parse_prompt(blocks: &[ContentBlock]) -> Result<(String, Vec<AgentImage>), String> {
    let mut payload = None;
    let mut images = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(text) if payload.is_none() => payload = Some(text.text.clone()),
            ContentBlock::Image(image) => images.push(AgentImage {
                data: image.data.clone(),
                mime_type: image.mime_type.clone(),
            }),
            _ => {}
        }
    }
    payload
        .map(|payload| (payload, images))
        .ok_or_else(|| "ACP 提示词缺少文本内容".into())
}

fn build_message(payload: &SolverPayload, images: &[AgentImage], include_images: bool) -> Message {
    let mut content = vec![UserContent::text(payload.model_input.clone())];
    if include_images {
        for (index, image) in images.iter().enumerate() {
            let label = payload
                .image_labels
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("[图片 {}/{}]", index + 1, images.len()));
            content.push(UserContent::text(label));
            content.push(UserContent::image_base64(
                image.data.clone(),
                ImageMediaType::from_mime_type(&image.mime_type),
                None,
            ));
        }
    }
    Message::User { content }
}

fn to_meta(outcome: &Outcome) -> serde_json::Map<String, Value> {
    serde_json::to_value(outcome)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::super::prompt::SolverOptions;
    use super::*;

    #[test]
    fn outcome_roundtrips_through_prompt_meta() {
        let outcome = Outcome {
            raw: "{\"answers\":{}}".into(),
            tool_log: "[工具调用]\n".into(),
            images_omitted: true,
            tools_disabled: false,
            error: None,
        };
        let meta = to_meta(&outcome);
        let decoded: Outcome = serde_json::from_value(Value::Object(meta)).expect("meta decodes");
        assert_eq!(decoded.raw, outcome.raw);
        assert!(decoded.images_omitted);
        assert!(!decoded.tools_disabled);
    }

    #[test]
    fn maps_reasoning_effort() {
        assert_eq!(parse_reasoning_effort("high"), Some(Effort::High));
        assert_eq!(parse_reasoning_effort("medium"), Some(Effort::Medium));
        assert_eq!(parse_reasoning_effort("low"), Some(Effort::Low));
        assert_eq!(parse_reasoning_effort("default"), None);
        assert_eq!(parse_reasoning_effort("none"), None);
        assert_eq!(parse_reasoning_effort(""), None);
    }

    #[test]
    fn build_message_skips_images_when_disabled() {
        let payload = SolverPayload {
            mode: SolverMode::SolveBatch,
            options: SolverOptions {
                include_reasoning: true,
                web_search: false,
                reasoning_effort: "default".into(),
            },
            image_labels: vec!["[题目 q1 图片 1/1]".into()],
            model_input: "{}".into(),
        };
        let images = vec![AgentImage {
            data: "AAAA".into(),
            mime_type: "image/png".into(),
        }];
        let with_images = build_message(&payload, &images, true);
        let without = build_message(&payload, &images, false);
        match (with_images, without) {
            (Message::User { content: a }, Message::User { content: b }) => {
                assert_eq!(a.len(), 3);
                assert_eq!(b.len(), 1);
            }
            _ => panic!("unexpected message shape"),
        }
    }
}
