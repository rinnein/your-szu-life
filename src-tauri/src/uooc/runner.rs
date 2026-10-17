use super::{
    client::{Client, Error},
    media,
    model::*,
    Service,
};
use serde_json::Value;
use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};
use tokio::{sync::Semaphore, task::JoinSet};
use tokio_util::sync::CancellationToken;

fn source(item: &Value) -> Option<String> {
    item["video_play_list"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(
            item["video_url"]
                .as_object()
                .into_iter()
                .flat_map(|map| map.values()),
        )
        .filter_map(|v| v["source"].as_str())
        .map(str::trim)
        .find(|s| !s.is_empty())
        .map(str::to_owned)
}
pub(super) fn collect_flat_catalog(
    nodes: &[Value],
    chapter_id: Option<&str>,
    out: &mut Vec<(Value, String, String)>,
) {
    for node in nodes {
        let current_id = id(&node["id"]).unwrap_or_default();
        let effective_chapter = chapter_id.unwrap_or(&current_id);

        let children = node["children"].as_array().filter(|c| !c.is_empty());
        let has_icons = node["icon_list"]
            .as_array()
            .map_or(false, |icons| !icons.is_empty());
        let has_task_id = id(&node["task_id"]).is_some_and(|tid| tid != "0");

        if children.is_none() || has_icons || has_task_id {
            out.push((node.clone(), effective_chapter.to_string(), current_id.clone()));
        }

        if let Some(children_arr) = children {
            collect_flat_catalog(children_arr, Some(effective_chapter), out);
        }
    }
}

pub(super) fn catalog_nodes(
    cid: &str,
    catalog: Vec<Value>,
) -> Result<VecDeque<(Value, Video)>, Error> {
    let mut queue = VecDeque::new();
    for chapter in catalog {
        let chapter_id = id(&chapter["id"]).ok_or(Error::Invalid)?;
        let children = chapter["children"]
            .as_array()
            .filter(|c| !c.is_empty())
            .cloned()
            .unwrap_or_else(|| vec![chapter]);
        for section in children {
            let section_id = id(&section["id"]).ok_or(Error::Invalid)?;
            queue.push_back((
                section,
                Video {
                    cid: cid.into(),
                    chapter: chapter_id.clone(),
                    section: section_id,
                    subsection: None,
                    resource: String::new(),
                    title: String::new(),
                    source: String::new(),
                    position: 0.0,
                    task_type: "video".into(),
                    task_id: None,
                },
            ));
        }
    }
    Ok(queue)
}
async fn discover(
    client: &Client,
    cid: &str,
    cancel: &CancellationToken,
) -> Result<(Vec<Video>, Vec<Video>, bool), Error> {
    let mut queue = catalog_nodes(cid, client.catalog(cid, cancel).await?)?;
    let mut videos = vec![];
    let mut completed = vec![];
    let mut seen = HashSet::new();
    let mut nodes = 0;
    while let Some((node, context)) = queue.pop_front() {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        nodes += 1;
        if nodes > 10000 {
            return Err(Error::Invalid);
        }

        // 识别任务类别：优先基于 icon_list[0] 的 type 和 type_name
        let first_icon = node["icon_list"].as_array().and_then(|arr| arr.first());
        let icon_type = first_icon.map(|i| text(&i["type"])).unwrap_or_default();
        let type_name = first_icon.map(|i| text(&i["type_name"])).unwrap_or_default();

        let is_quiz_node = icon_type == "80"
            || type_name.contains("测验")
            || (id(&node["task_id"]).is_some_and(|tid| tid != "0") && text(&node["name"]).contains("测验"));

        if is_quiz_node {
            let quiz_task_id = id(&node["task_id"]).filter(|tid| tid != "0");
            let quiz_res_id = first_icon
                .and_then(|i| id(&i["id"]))
                .unwrap_or_else(|| id(&node["id"]).unwrap_or_else(|| context.section.clone()));

            if seen.insert(quiz_res_id.clone()) {
                let quiz_video = Video {
                    resource: quiz_res_id,
                    title: text(&node["name"]),
                    source: String::new(),
                    position: 0.0,
                    task_type: "quiz".into(),
                    task_id: quiz_task_id,
                    ..context.clone()
                };
                if number(&node["finished"]) != 0.0 {
                    completed.push(quiz_video);
                }
            }
        } else {
            // 视频任务：调用 unit 查询单元项（涵盖视频和单元内嵌测验）
            let items = match client.unit(&context, cancel).await {
                Ok(items) => items,
                Err(Error::Gate) => return Ok((videos, completed, true)),
                Err(error) => return Err(error),
            };
            for item in items {
                let Some(resource) = id(&item["id"]) else {
                    continue;
                };
                if !seen.insert(resource.clone()) {
                    continue;
                }
                let src = source(&item).unwrap_or_default();
                let is_finished = number(&item["finished"]) != 0.0;
                let item_type = number(&item["type"]);
                let is_quiz = item_type == 80.0;
                let tid = id(&item["task_id"]);
                let is_video_type = item_type == 10.0 || !src.is_empty() || item.get("video_pos").is_some();
                if is_quiz || is_video_type || is_finished {
                    let video = Video {
                        resource,
                        title: text(&item["title"]),
                        source: src,
                        position: number(&item["video_pos"]).max(0.0),
                        task_type: if is_quiz { "quiz".into() } else { "video".into() },
                        task_id: tid,
                        ..context.clone()
                    };
                    if is_finished {
                        completed.push(video);
                    } else if !is_quiz && !video.source.is_empty() {
                        videos.push(video);
                    }
                }
            }
        }

        // 递归遍历子节点 (subsections)
        if let Some(children) = node["children"].as_array() {
            for child in children {
                let mut child_context = context.clone();
                if let Some(sub_id) = id(&child["id"]) {
                    if child_context.subsection.is_none() {
                        child_context.subsection = Some(sub_id);
                    } else {
                        child_context.section = child_context.subsection.unwrap();
                        child_context.subsection = Some(sub_id);
                    }
                }
                queue.push_back((child.clone(), child_context));
            }
        }
    }
    Ok((videos, completed, false))
}

pub(super) async fn run(
    service: Arc<Service>,
    client: Arc<Client>,
    cid: String,
    generation: u64,
    settings: Settings,
    permits: Arc<Semaphore>,
    cancel: CancellationToken,
) -> Result<TaskStatus, Error> {
    if service
        .fetch_progress(&cid, &cancel)
        .await?
        .video_complete()
    {
        // 如果视频已全部刷完，尝试同步平台侧已完成的任务项，使任务列表中完整按顺序展示所有任务项
        if let Ok((_pending, completed, _gate)) = discover(&client, &cid, &cancel).await {
            service.task_change(&cid, generation, |task| {
                for video in completed {
                    let duration = service.durations.cached(&cid, &video.resource);
                    let state = VideoState {
                        resource_id: video.resource.clone(),
                        title: video.title,
                        status: TaskStatus::Completed,
                        position: video.position,
                        duration,
                        message: String::new(),
                        task_type: Some(video.task_type.clone()),
                        task_id: video.task_id.clone(),
                    };
                    if let Some(old) = task
                        .videos
                        .iter_mut()
                        .find(|v| v.resource_id == video.resource)
                    {
                        old.status = TaskStatus::Completed;
                        old.duration = old.duration.or(duration);
                        old.task_type = old.task_type.clone().or_else(|| Some(video.task_type.clone()));
                        old.task_id = old.task_id.clone().or_else(|| video.task_id.clone());
                        old.message.clear();
                    } else {
                        task.videos.push(state);
                    }
                }
            });
        }
        return super::quiz::run(service, client, cid, generation, false, cancel).await;
    }
    service.task_change(&cid, generation, |t| t.status = TaskStatus::Running);
    let mut processed = HashSet::new();
    let mut failed = false;
    let mut video_gate = false;
    loop {
        let (videos, completed, gate) = discover(&client, &cid, &cancel).await?;
        service.task_change(&cid, generation, |task| {
            for video in completed {
                let duration = service.durations.cached(&cid, &video.resource);
                let state = VideoState {
                    resource_id: video.resource.clone(),
                    title: video.title,
                    status: TaskStatus::Completed,
                    position: video.position,
                    duration,
                    message: String::new(),
                    task_type: Some(video.task_type.clone()),
                    task_id: video.task_id.clone(),
                };
                if let Some(old) = task
                    .videos
                    .iter_mut()
                    .find(|v| v.resource_id == video.resource)
                {
                    // A sparse server response must not erase the last position confirmed in this run.
                    old.status = TaskStatus::Completed;
                    old.duration = old.duration.or(duration);
                    old.task_type = old.task_type.clone().or_else(|| Some(video.task_type.clone()));
                    old.task_id = old.task_id.clone().or_else(|| video.task_id.clone());
                    old.message.clear();
                } else {
                    task.videos.push(state);
                }
            }
        });
        let mut pending: VecDeque<_> = videos
            .into_iter()
            .filter(|v| processed.insert(v.resource.clone()))
            .collect();
        if pending.is_empty() {
            if !failed && !settings.dry_run {
                let result = super::quiz::run(
                    service.clone(),
                    client.clone(),
                    cid.clone(),
                    generation,
                    gate || video_gate,
                    cancel.clone(),
                )
                .await?;
                if result == TaskStatus::Queued {
                    processed.clear();
                    video_gate = false;
                    continue;
                }
                return Ok(result);
            }
            return Ok(if gate || video_gate {
                TaskStatus::Blocked
            } else if failed {
                TaskStatus::Failed
            } else if settings.dry_run {
                TaskStatus::DryRun
            } else {
                TaskStatus::Completed
            });
        }
        service.task_change(&cid, generation, |task| {
            for v in &pending {
                let next = VideoState {
                    resource_id: v.resource.clone(),
                    title: v.title.clone(),
                    status: TaskStatus::Queued,
                    position: v.position,
                    duration: service.durations.cached(&cid, &v.resource),
                    message: String::new(),
                    task_type: Some(v.task_type.clone()),
                    task_id: v.task_id.clone(),
                };
                if let Some(old) = task
                    .videos
                    .iter_mut()
                    .find(|row| row.resource_id == v.resource)
                {
                    *old = next;
                } else {
                    task.videos.push(next);
                }
            }
        });
        let mut running = JoinSet::new();
        while !pending.is_empty() || !running.is_empty() {
            // At most one outstanding permit request per course. Tokio's FIFO semaphore
            // lets other courses obtain the next slot instead of queuing an entire course first.
            tokio::select! { biased;
                _ = cancel.cancelled() => return Err(Error::Cancelled),
                result = running.join_next(), if !running.is_empty() => {
                    match result {
                        Some(Ok(Ok(TaskStatus::Blocked))) => video_gate = true,
                        Some(Ok(Ok(TaskStatus::Failed))) => failed = true,
                        Some(Ok(Err(Error::Unauthorized))) => return Err(Error::Unauthorized),
                        Some(Ok(Err(Error::Cancelled))) => return Err(Error::Cancelled),
                        Some(Err(_)) | Some(Ok(Err(_))) => failed = true,
                        _ => {},
                    }
                },
                permit = permits.clone().acquire_owned(), if !pending.is_empty() => {
                    let permit = permit.map_err(|_| Error::Cancelled)?;
                    let video = pending.pop_front().unwrap();
                    let service = service.clone(); let client = client.clone(); let cancel = cancel.clone(); let settings = settings.clone();
                    running.spawn(async move { let _permit = permit; process_video(service, client, video, generation, settings, cancel).await });
                    tokio::task::yield_now().await;
                }
            }
        }
        if settings.dry_run {
            return Ok(if failed {
                TaskStatus::Failed
            } else if gate {
                TaskStatus::Blocked
            } else {
                TaskStatus::DryRun
            });
        }
    }
}

async fn process_video(
    service: Arc<Service>,
    client: Arc<Client>,
    video: Video,
    generation: u64,
    settings: Settings,
    cancel: CancellationToken,
) -> Result<TaskStatus, Error> {
    let update = |status, position, duration, message: String| {
        service.task_change(&video.cid, generation, |task| {
            if let Some(state) = task
                .videos
                .iter_mut()
                .find(|v| v.resource_id == video.resource)
            {
                state.status = status;
                state.position = position;
                state.duration = duration;
                state.message = message;
            }
        })
    };
    update(
        TaskStatus::Running,
        video.position,
        None,
        "正在读取视频时长缓存…".into(),
    );
    let duration = match service
        .durations
        .get_or_probe(&video.cid, &video.resource, &cancel, || {
            media::probe(video.source.clone(), cancel.clone())
        })
        .await
    {
        Ok(duration) => duration,
        Err(message) => {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            update(
                TaskStatus::Failed,
                video.position,
                None,
                format!("跳过：{message}"),
            );
            return Ok(TaskStatus::Failed);
        }
    };
    let mut position = if video.position > 0.0 {
        video.position + 1.0
    } else {
        0.0
    };
    position = position.min(duration - 1.0);
    update(TaskStatus::Running, position, Some(duration), String::new());
    if settings.dry_run {
        update(
            TaskStatus::DryRun,
            position,
            Some(duration),
            "预览完成，未上报视频进度".into(),
        );
        return Ok(TaskStatus::DryRun);
    }
    let result = async {
        let mut request = 0;
        let mut tail = 0;
        let mut uncertain = 0;
        loop {
            if cancel.is_cancelled() { return Err(Error::Cancelled); }
            request += 1;
            if request > 1 { position = (position + settings.report_interval_sec as f64 * multiplier(request, settings.speed_multiplier)).min(duration - 1.0); }
            match client.mark(&video, duration, position, &cancel).await {
                Ok(done) => {
                    {
                        let runtime = service.runtime.lock().unwrap();
                        if runtime.workers.get(&video.cid).map(|(g, _)| *g) == Some(generation) { let _ = service.save_session(&client); }
                    }
                    if done { return Ok(TaskStatus::Completed); }
                    update(TaskStatus::Running, position, Some(duration), String::new());
                    if position >= duration - 1.0 { tail += 1; if tail >= 3 { return Err(Error::Server(-1)); } }
                }
                Err(Error::Gate) => return Ok(TaskStatus::Blocked),
                Err(Error::Network | Error::Invalid) => {
                    uncertain += 1;
                    let rows = client.unit(&video, &cancel).await?;
                    let current = rows.iter().find(|row| id(&row["id"]).as_deref() == Some(&video.resource)).ok_or(Error::Invalid)?;
                    if number(&current["finished"]) != 0.0 { return Ok(TaskStatus::Completed); }
                    if uncertain >= 3 { return Err(Error::Network); }
                    position = number(&current["video_pos"]).max(0.0).min(duration - 1.0);
                    // The next report starts at the observed server position, not an assumed step.
                    request = 0;
                    update(TaskStatus::Running, position, Some(duration), "上报结果不确定，已重新读取服务端进度".into());
                }
                Err(error) => return Err(error),
            }
            tokio::select! { biased; _ = cancel.cancelled() => return Err(Error::Cancelled), _ = tokio::time::sleep(Duration::from_secs(settings.report_interval_sec)) => {} }
        }
    }.await;
    match &result {
        Ok(status) => update(
            status.clone(),
            position,
            Some(duration),
            if *status == TaskStatus::Blocked {
                "需要完成前置任务".into()
            } else {
                String::new()
            },
        ),
        Err(error) => update(
            TaskStatus::Failed,
            position,
            Some(duration),
            if *error == Error::Server(-1) {
                "已到达视频尾部，但服务器尚未确认完成".into()
            } else {
                error.message()
            },
        ),
    }
    result
}
