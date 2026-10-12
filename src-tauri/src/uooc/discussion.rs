use super::{
    client::{Client, Error},
    model::{DiscussionStatus, SessionStatus},
    progress::CourseProgress,
    Service,
};
use chrono::Utc;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub(super) fn cooldown_until(last_sent_at_ms: Option<i64>, settings: &super::model::Settings) -> Option<i64> {
    last_sent_at_ms.map(|at| {
        at.saturating_add((settings.discussion_interval_min as i64).saturating_mul(60_000))
    })
}

fn now() -> i64 {
    Utc::now().timestamp_millis()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn eligible(progress: &CourseProgress) -> bool {
    progress
        .discussion_completion_ratio()
        .is_some_and(|ratio| ratio < 1.0)
}

fn update(service: &Service, cid: &str, status: DiscussionStatus, message: &str) {
    let mut runtime = service.runtime.lock().unwrap();
    let entry = runtime
        .discussion
        .courses
        .entry(cid.to_owned())
        .or_default();
    entry.status = status;
    entry.message = message.to_owned();
    entry.updated_at_ms = now();
    service.publish(&mut runtime);
}

fn current(service: &Service, account: &str, cid: &str) -> bool {
    let runtime = service.runtime.lock().unwrap();
    runtime.snapshot.session == SessionStatus::Ready
        && runtime
            .snapshot
            .user
            .as_ref()
            .is_some_and(|user| user.id == account)
        && runtime
            .snapshot
            .courses
            .iter()
            .any(|course| course.id == cid)
}

fn cooldown_deadline(service: &Service) -> Option<i64> {
    let runtime = service.runtime.lock().unwrap();
    cooldown_until(
        runtime.discussion.last_sent_at_ms,
        &runtime.snapshot.settings,
    )
}

fn remembered_tids(service: &Service, cid: &str) -> Vec<String> {
    service
        .runtime
        .lock()
        .unwrap()
        .discussion
        .courses
        .get(cid)
        .map(|state| {
            let mut tids = state.processed_tids.clone();
            if let Some(tid) = &state.last_tid {
                if !tids.contains(tid) {
                    tids.push(tid.clone());
                }
            }
            tids
        })
        .unwrap_or_default()
}

fn set_success(
    service: &Service,
    cid: &str,
    tid: Option<String>,
    completion_ratio: Option<f64>,
    message: &str,
) {
    let mut runtime = service.runtime.lock().unwrap();
    runtime.discussion.last_sent_at_ms = Some(now());
    let entry = runtime
        .discussion
        .courses
        .entry(cid.to_owned())
        .or_default();
    entry.status = DiscussionStatus::Completed;
    entry.message = message.to_owned();
    entry.last_tid = tid;
    entry.last_completion_ratio = completion_ratio;
    entry.last_sent_at_ms = Some(now());
    if let Some(tid) = &entry.last_tid {
        if !entry.processed_tids.contains(tid) {
            entry.processed_tids.push(tid.clone());
        }
    }
    entry.updated_at_ms = now();
    service.publish(&mut runtime);
}

fn set_failed(service: &Service, cid: &str, message: String) {
    update(service, cid, DiscussionStatus::Failed, &message);
}

async fn execute(
    service: &Arc<Service>,
    client: &Client,
    account: &str,
    cid: &str,
    content: &str,
    cancel: &CancellationToken,
) -> Result<(), Error> {
    if !current(service, account, cid) {
        return Err(Error::Cancelled);
    }
    let remembered = remembered_tids(service, cid);
    let threads = client.thread_list(cid, cancel).await?;
    if let Some(thread) = threads
        .iter()
        .find(|thread| !remembered.iter().any(|tid| tid == &thread.tid))
        .cloned()
    {
        update(service, cid, DiscussionStatus::Sending, "正在回复讨论帖");
        let reply = client.thread_details(cid, &thread.tid, cancel).await?;
        if reply.trim().is_empty() {
            return Err(Error::Invalid);
        }
        client
            .reply_thread(cid, &thread.tid, &reply, cancel)
            .await?;
        let completion_ratio = service
            .snapshot()
            .progress
            .get(cid)
            .and_then(|entry| entry.data.as_ref())
            .and_then(CourseProgress::discussion_completion_ratio);
        set_success(
            service,
            cid,
            Some(thread.tid),
            completion_ratio,
            "已回复讨论帖",
        );
    } else if threads.is_empty() {
        update(service, cid, DiscussionStatus::Sending, "正在发表讨论帖");
        let post_content = format!("<p>{}</p>", escape_html(content.trim()));
        client
            .post_thread(cid, &post_content, "要好好学习", cancel)
            .await?;
        let completion_ratio = service
            .snapshot()
            .progress
            .get(cid)
            .and_then(|entry| entry.data.as_ref())
            .and_then(CourseProgress::discussion_completion_ratio);
        set_success(service, cid, None, completion_ratio, "已发表讨论帖");
    } else {
        update(
            service,
            cid,
            DiscussionStatus::Skipped,
            "讨论帖已处理，等待服务端进度更新",
        );
    }
    Ok(())
}

impl Service {
    pub(super) async fn start_discussion_once(self: &Arc<Self>, cid: &str) -> Result<(), String> {
        let _lock = self
            .discussion_lock
            .try_lock()
            .map_err(|_| "讨论任务正在运行，请稍后再试".to_owned())?;
        let snapshot = self.snapshot();
        if snapshot.session != SessionStatus::Ready {
            return Err("请先登录 UOOC".into());
        }
        let course = snapshot
            .courses
            .iter()
            .find(|course| course.id == cid)
            .ok_or("只能为当前开课中的课程参与讨论")?;
        let progress = snapshot
            .progress
            .get(cid)
            .and_then(|entry| entry.data.as_ref())
            .ok_or("课程进度尚未读取完成，请先刷新课程")?;
        if !eligible(progress) {
            return Err("该课程没有待完成的讨论任务".into());
        }
        let account = snapshot.user.as_ref().ok_or("请先登录 UOOC")?.id.clone();
        update(self, cid, DiscussionStatus::Eligible, "准备参与一次讨论");
        let cancel = CancellationToken::new();
        let client = self.client();
        execute(
            self,
            &client,
            &account,
            &course.id,
            &snapshot.settings.discussion_content,
            &cancel,
        )
        .await
        .map_err(|error| {
            if error != Error::Cancelled {
                set_failed(self, cid, error.message());
            }
            error.message()
        })
    }

    pub(super) async fn run_discussions(self: &Arc<Self>) {
        let Ok(_lock) = self.discussion_lock.try_lock() else {
            return;
        };
        let snapshot = self.snapshot();
        if snapshot.session != SessionStatus::Ready || !snapshot.settings.discussion_enabled {
            return;
        }
        let current_time = now();
        if snapshot
            .discussion_cooldown_until_ms
            .is_some_and(|until| until > current_time)
        {
            return;
        }
        let Some(account) = snapshot.user.as_ref().map(|user| user.id.clone()) else {
            return;
        };
        let cancel = CancellationToken::new();
        for course in snapshot.courses {
            if !current(self, &account, &course.id) {
                continue;
            }
            let Some(progress) = snapshot
                .progress
                .get(&course.id)
                .and_then(|entry| entry.data.as_ref())
            else {
                continue;
            };
            if !eligible(progress) {
                if progress
                    .discussion_completion_ratio()
                    .is_some_and(|ratio| ratio >= 1.0)
                {
                    let mut runtime = self.runtime.lock().unwrap();
                    runtime.discussion.courses.remove(&course.id);
                    self.publish(&mut runtime);
                    continue;
                }
                update(
                    self,
                    &course.id,
                    DiscussionStatus::Skipped,
                    "讨论权重为零或完成率未知",
                );
                continue;
            }
            if snapshot.discussions.get(&course.id).is_some_and(|state| {
                state.status == DiscussionStatus::Completed
                    && state.last_completion_ratio == progress.discussion_completion_ratio()
            }) {
                continue;
            }
            update(self, &course.id, DiscussionStatus::Eligible, "等待讨论发送");
            let client = self.client();
            let result = execute(
                self,
                &client,
                &account,
                &course.id,
                &snapshot.settings.discussion_content,
                &cancel,
            )
            .await;
            if let Err(error) = result {
                if error != Error::Cancelled {
                    set_failed(self, &course.id, error.message());
                }
            }
            if cooldown_deadline(self).is_some_and(|until| until > now()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uooc::client::Client;
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    fn read_request(stream: &mut TcpStream) -> Option<(String, String)> {
        let mut bytes = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let n = stream.read(&mut chunk).ok()?;
            if n == 0 {
                return None;
            }
            bytes.extend_from_slice(&chunk[..n]);
            let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") else {
                continue;
            };
            let header = String::from_utf8_lossy(&bytes[..end]);
            let length = header
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .and_then(|v| v.trim().parse().ok())
                })
                .unwrap_or(0);
            if bytes.len() < end + 4 + length {
                continue;
            }
            let mut first = header.lines().next()?.split_whitespace();
            let method = first.next()?.to_owned();
            let path = first.next()?.to_owned();
            return Some((method, path));
        }
    }

    #[test]
    fn eligibility_requires_positive_weight_and_known_incomplete_percent() {
        let mut progress = CourseProgress::parse(
            &json!({"discuz":5,"discuss_score":2.5}),
            super::super::progress::today(),
        );
        assert!(eligible(&progress));
        assert_eq!(progress.discussion_completion_ratio(), Some(0.5));
        progress.discussion.score = Some(5.0);
        assert!(!eligible(&progress));
        progress.discussion.score = None;
        assert!(!eligible(&progress));
        progress.discussion.weight = Some(0.0);
        progress.discussion.score = Some(50.0);
        assert!(!eligible(&progress));
    }

    #[test]
    fn post_content_is_html_escaped_and_wrapped() {
        assert_eq!(
            format!("<p>{}</p>", escape_html("<学习> & \"now\"")),
            "<p>&lt;学习&gt; &amp; &quot;now&quot;</p>"
        );
    }

    #[tokio::test]
    async fn thread_endpoints_use_required_routes_and_fields() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let observed = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = observed.clone();
        let worker = thread::spawn(move || {
            for _ in 0..4 {
                loop {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let request = read_request(&mut stream).unwrap();
                            seen.lock().unwrap().push(request.1);
                            let body = if seen.lock().unwrap().len() == 1 {
                                br#"{"code":1,"data":[{"tid":"9"}]}"#.to_vec()
                            } else if seen.lock().unwrap().len() == 2 {
                                br#"{"code":1,"data":{"content":"hello"}}"#.to_vec()
                            } else {
                                br#"{"code":1,"data":{}}"#.to_vec()
                            };
                            let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n", body.len());
                            stream.write_all(response.as_bytes()).unwrap();
                            stream.write_all(&body).unwrap();
                            break;
                        }
                        Err(_) => thread::sleep(Duration::from_millis(1)),
                    }
                }
            }
        });
        let client = Client::with_base(crate::uooc::session::load(None).unwrap(), &base).unwrap();
        let cancel = CancellationToken::new();
        assert_eq!(client.thread_list("42", &cancel).await.unwrap()[0].tid, "9");
        assert_eq!(
            client.thread_details("42", "9", &cancel).await.unwrap(),
            "hello"
        );
        client
            .reply_thread("42", "9", "hello", &cancel)
            .await
            .unwrap();
        client
            .post_thread("42", "<p>x</p>", "要好好学习", &cancel)
            .await
            .unwrap();
        worker.join().unwrap();
        let requests = observed.lock().unwrap();
        assert!(requests
            .iter()
            .any(|path| path.starts_with("/Home/Threads/list?")));
        assert!(requests
            .iter()
            .any(|path| path.starts_with("/Home/Threads/details?")));
        assert!(requests.iter().any(|path| path == "/Home/Threads/reply"));
        assert!(requests.iter().any(|path| path == "/Home/Threads/post"));
    }
}
