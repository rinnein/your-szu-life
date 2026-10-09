use super::*;
use sha2::{Digest, Sha256};

pub(super) fn target(sites: &[Site], raw: &str) -> Result<(String, String, String), String> {
    if raw.len() > 4096 {
        return Err("题目链接过长".into());
    }
    let mut url = reqwest::Url::parse(raw).map_err(|_| "题目链接无效")?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("题目链接不能包含凭据".into());
    }
    let fragment = url
        .fragment()
        .ok_or("请使用从泥深发送到 cph-ng 的题目链接")?
        .to_owned();
    url.set_fragment(None);
    let site = sites
        .iter()
        .find(|s| s.url == url.as_str())
        .ok_or("题目链接不属于已配置的 OJ 实例")?;
    let route =
        reqwest::Url::parse(&format!("http://oj.invalid{fragment}")).map_err(|_| "题目路径无效")?;
    if route.host_str() != Some("oj.invalid") {
        return Err("题目路径无效".into());
    }
    let parts: Vec<_> = route.path().trim_matches('/').split('/').collect();
    if parts.len() != 2 || parts[0] != "contest" {
        return Err("题目链接缺少测验信息".into());
    }
    let cid = protocol::id(&json!(parts[1])).map_err(|e| e.message)?;
    let query: Vec<_> = route.query_pairs().collect();
    if query.len() != 2 {
        return Err("题目链接参数无效".into());
    }
    let pid = query
        .iter()
        .find(|(k, _)| k == "problem")
        .ok_or("题目编号缺失")?;
    let kind = query.iter().find(|(k, _)| k == "type").ok_or("题型缺失")?;
    let pid = protocol::id(&json!(pid.1)).map_err(|e| e.message)?;
    if !matches!(kind.1.as_ref(), "0" | "4") {
        return Err("cph-ng 仅能提交编程题".into());
    }
    Ok((site.id.clone(), cid, format!("{}:{pid}", kind.1)))
}
impl Service {
    pub(super) fn configure_companion(&self, enabled: bool, port: u16) -> Result<(), String> {
        if cfg!(mobile) {
            return Err("手机端不提供 cph-ng 联动".into());
        }
        if port == 0 {
            return Err("本机接收端口无效".into());
        }
        let mut r = self.runtime.lock().unwrap();
        let changed = r.data.companion_enabled != enabled || r.data.companion_port != port;
        let mut data = r.data.clone();
        data.companion_enabled = enabled;
        data.companion_port = port;
        persistence::write(&self.path, &data)?;
        r.data = data;
        if changed {
            r.companion_status = CompanionStatus::default();
        }
        drop(r);
        self.publish();
        Ok(())
    }
    pub(super) fn companion_status(&self, port: u16, status: CompanionStatus) {
        let mut r = self.runtime.lock().unwrap();
        if r.data.companion_port != port || (!r.data.companion_enabled && status.connected) {
            return;
        }
        r.companion_status = status;
        drop(r);
        self.publish();
    }
    pub(super) async fn companion_submit(
        &self,
        request: CompanionSubmission,
    ) -> Result<CompanionResult, String> {
        if cfg!(mobile) {
            return Err("手机端不提供 cph-ng 联动".into());
        }
        if request.source_code.trim().is_empty() || request.source_code.len() > 512 * 1024 {
            return Err("代码为空或超过 512 KB".into());
        }
        let (id, cid, pk) = {
            let r = self.runtime.lock().unwrap();
            if !r.data.companion_enabled || !r.companion_status.active {
                return Err("泥深尚未启用为 cph-ng 提交端".into());
            }
            target(&r.data.sites, &request.url)?
        };
        let e = self.entry(&id)?;
        let now = chrono::Utc::now().timestamp_millis();
        let settings_revision =
            Self::preferences_locked(&self.runtime.lock().unwrap(), &id, &e.username).revision;
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(
                format!(
                    "{}\0{}\0{}\0{}",
                    e.epoch, settings_revision, request.url, request.source_code
                )
                .as_bytes()
            )
        );
        {
            let mut r = self.runtime.lock().unwrap();
            r.companion_requests.retain(|_, time| now - *time < 30_000);
            if r.companion_requests.contains_key(&fingerprint) {
                return Err("相同代码刚刚已收到提交请求，请先查看提交结果".into());
            }
            r.companion_requests.insert(fingerprint.clone(), now);
        }
        let result = self
            .submit_with_code(
                SubmitInput {
                    site_id: id.clone(),
                    contest_id: cid.clone(),
                    problem_key: Some(pk.clone()),
                    account: e.username.clone(),
                    epoch: e.epoch,
                },
                Some(request.source_code),
            )
            .await;
        if result.is_err() {
            let mut r = self.runtime.lock().unwrap();
            if !r
                .data
                .uncertain
                .contains_key(&operation_key(&id, &e.username, &cid, Some(&pk)))
            {
                r.companion_requests.remove(&fingerprint);
            }
        }
        let message = match &result {
            Ok(v) => format!("cph-ng 代码已提交，提交编号 {}", v["id"]),
            Err(error) => format!("cph-ng 提交未完成：{error}"),
        };
        let notice = Notice {
            id: uuid::Uuid::new_v4().to_string(),
            site_id: id.clone(),
            account: e.username.clone(),
            contest_id: cid.clone(),
            title: "OJ · cph-ng 提交".into(),
            message: message.clone(),
            timestamp: now,
            read: false,
        };
        {
            let mut r = self.runtime.lock().unwrap();
            r.companion_status.message = message;
            r.data.notices.push(notice.clone());
            if r.data.notices.len() > 500 {
                let count = r.data.notices.len() - 500;
                r.data.notices.drain(..count);
            }
            let _ = self.persist(&r);
        }
        self.publish();
        self.notify(&notice);
        let submission = result?;
        let outcome = CompanionResult {
            site_id: id,
            contest_id: cid,
            problem_key: pk,
            account: e.username.clone(),
            epoch: e.epoch,
            submission,
        };
        if let Some(app) = &self.app {
            let _ = app.emit("oj-companion-result", outcome.clone());
        }
        Ok(outcome)
    }
}
