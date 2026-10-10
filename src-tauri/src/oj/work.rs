use super::*;
impl Service {
    pub(super) async fn progress(
        &self,
        id: &str,
        cid: &str,
        force: bool,
        account: Option<&str>,
        generation: Option<u64>,
    ) -> Result<Value, String> {
        protocol::id(&json!(cid)).map_err(|e| e.message)?;
        let e = self.entry(id)?;
        if account.is_some_and(|a| a != e.username) || generation.is_some_and(|g| g != e.epoch) {
            return Err("OJ 账号配置已变化，请重新加载".into());
        }
        let cached = || {
            let now = chrono::Utc::now().timestamp_millis();
            e.state
                .lock()
                .unwrap()
                .progress
                .get(cid)
                .filter(|(time, _)| !force && (0..3_600_000).contains(&(now - time)))
                .map(|(_, value)| value.clone())
        };
        if !self.current(id, &e) {
            return Err("OJ 账号配置已变化，请重新加载".into());
        }
        if let Some(value) = cached() {
            return Ok(value);
        }
        let _guard = e.gate.lock().await;
        if !self.current(id, &e) {
            return Err("OJ 账号配置已变化，请重新加载".into());
        }
        // Another WebView may have completed the same request while we waited.
        if let Some(value) = cached() {
            return Ok(value);
        }
        self.ensure(id, &e).await?;
        let raw = self.read(id, &e, &format!("contests/{cid}/"), &[]).await?;
        let problems = protocol::problems(&raw).map_err(|e| e.message)?;
        let rank = self
            .read(id, &e, &format!("contests/{cid}/self_rank/"), &[])
            .await?;
        let mut accepted = problems
            .iter()
            .filter(|p| {
                p.kind == 0
                    && rank["data"]["submission_info"][&p.id]["accepted"].as_bool() == Some(true)
            })
            .count();
        if problems.iter().any(|p| p.kind == 4) {
            let rows = self
                .list(
                    id,
                    &e,
                    "code-fill-submission/my_submissions/",
                    &[("contest", cid.into())],
                )
                .await?;
            accepted += problems
                .iter()
                .filter(|p| {
                    p.kind == 4
                        && rows.iter().any(|r| {
                            r["status"].as_i64() == Some(0)
                                && protocol::record_for_problem(r, 4, &p.id).is_some()
                        })
                })
                .count();
        }
        let now = chrono::Utc::now().timestamp_millis();
        let value = json!({"accepted":accepted,"total":problems.len(),"updatedAt":now});
        e.state
            .lock()
            .unwrap()
            .progress
            .insert(cid.into(), (now, value.clone()));
        Ok(value)
    }

    fn invalidate_progress(&self, id: &str, e: &Entry, cid: &str) {
        e.state.lock().unwrap().progress.remove(cid);
        if let Some(app) = &self.app {
            let _ = app.emit("oj-progress-invalidated", json!({"siteId":id,"account":e.username,"sessionGeneration":e.epoch,"contestId":cid}));
        }
    }

    pub(super) async fn ranking(&self, id: &str, cid: &str, offset: u32) -> Result<Value, String> {
        protocol::id(&json!(cid)).map_err(|e| e.message)?;
        if offset > 100_000 {
            return Err("排名分页超出范围".into());
        }
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        let contest = self.read(id, &e, &format!("contests/{cid}/"), &[]).await?;
        if contest["show_ranking"].as_bool() == Some(false) {
            return Err("此测验未开放排名".into());
        }
        let raw = self
            .read(
                id,
                &e,
                &format!("contests/{cid}/rank/"),
                &[("offset", offset.to_string()), ("limit", "20".into())],
            )
            .await?;
        protocol::ranking(&raw, offset).map_err(|e| e.message)
    }

    async fn paper_locked(&self, id: &str, e: &Entry, cid: &str) -> Result<Paper, String> {
        protocol::id(&json!(cid)).map_err(|e| e.message)?;
        self.ensure(id, e).await?;
        let raw = self.read(id, e, &format!("contests/{cid}/"), &[]).await?;
        let contest = protocol::contest(&raw).map_err(|e| e.message)?;
        let problems = protocol::problems(&raw).map_err(|e| e.message)?;
        let submissions = e
            .client
            .list("contest-submission/", &[("contest", cid.into())])
            .await
            .map_err(|e| e.message)?;
        let submitted = if let Some(row) = submissions.first() {
            let sid = protocol::id(&row["id"]).map_err(|e| e.message)?;
            Some(
                self.read(id, e, &format!("contest-submission/{sid}/"), &[])
                    .await?,
            )
        } else {
            None
        };
        let qa = if problems.iter().any(|p| p.kind == 3) {
            self.list(
                id,
                e,
                "qa-submission/my_submissions/",
                &[("contest", cid.into())],
            )
            .await?
        } else {
            vec![]
        };
        let r = self.runtime.lock().unwrap();
        let preferences = Self::preferences_locked(&r, id, &e.username);
        let mut drafts = BTreeMap::new();
        for p in &problems {
            let pk = problem_key(p);
            let selected_key = draft_key(id, &e.username, cid, &pk, "");
            let mut draft = r
                .data
                .drafts
                .get(&selected_key)
                .cloned()
                .unwrap_or_else(|| {
                    let answer = match p.kind {
                        1 => submitted
                            .as_ref()
                            .map(|s| s["choice_submission"][&p.id].clone())
                            .unwrap_or(json!([])),
                        2 => submitted
                            .as_ref()
                            .map(|s| s["fill_in_submission"][&p.id].clone())
                            .unwrap_or(json!([])),
                        3 => qa
                            .iter()
                            .find(|a| protocol::id(&a["problem"]).ok().as_deref() == Some(&p.id))
                            .map(|a| a["answer"].clone())
                            .unwrap_or(json!("")),
                        _ => json!(""),
                    };
                    Draft {
                        language: p.languages.first().cloned().unwrap_or_default(),
                        answer: if answer.is_null() { json!([]) } else { answer },
                        ..Default::default()
                    }
                });
            if matches!(p.kind, 0 | 4) {
                let language = &preferences.language;
                draft = r
                    .data
                    .drafts
                    .get(&draft_key(id, &e.username, cid, &pk, language))
                    .cloned()
                    .unwrap_or_else(|| {
                        if draft.language == *language {
                            draft.clone()
                        } else {
                            Draft::default()
                        }
                    });
                draft.language = language.clone();
                draft.options = preferences.options()?;
            }
            if draft.code.is_empty() {
                draft.code = p
                    .templates
                    .get(&draft.language)
                    .cloned()
                    .unwrap_or_default()
            }
            draft.options = CompileOptions::for_language(&draft.language, draft.options.as_ref())?;
            drafts.insert(pk, draft);
        }
        if r.entries
            .get(id)
            .is_none_or(|current| current.epoch != e.epoch)
        {
            return Err("账号已变化，请重新加载".into());
        }
        let now = contest.now;
        let writable = contest.phase(now) == 1
            && raw["finished"].as_bool() != Some(true)
            && submitted.as_ref().is_none_or(|s| {
                s["judged"].as_bool() != Some(true) && s["finished"].as_bool() != Some(true)
            });
        let base = r
            .data
            .sites
            .iter()
            .find(|s| s.id == id)
            .ok_or("OJ 不存在")?
            .url
            .clone();
        Ok(Paper {
            contest,
            problems,
            drafts,
            account: e.username.clone(),
            epoch: e.epoch,
            writable,
            submitted,
            source_url: protocol::source_url(&base, cid, None),
            preferences,
        })
    }
    pub(super) async fn paper(&self, id: &str, cid: &str) -> Result<Paper, String> {
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.paper_locked(id, &e, cid).await
    }
    pub(super) fn save_draft(&self, mut input: DraftInput) -> Result<(), String> {
        input.draft.options =
            CompileOptions::for_language(&input.draft.language, input.draft.options.as_ref())?;
        if serde_json::to_vec(&input.draft)
            .map_err(|_| "草稿格式无效")?
            .len()
            > 512 * 1024
        {
            return Err("草稿超过 512 KB".into());
        }
        protocol::id(&json!(input.contest_id)).map_err(|e| e.message)?;
        let mut r = self.runtime.lock().unwrap();
        let e = r.entries.get(&input.site_id).ok_or("OJ 未配置")?;
        if e.epoch != input.epoch || e.username != input.account {
            return Err("账号已变化，未保存到其它账号".into());
        }
        let key = draft_key(
            &input.site_id,
            &input.account,
            &input.contest_id,
            &input.problem_key,
            &input.draft.language,
        );
        if let Some(previous) = r.data.drafts.get(&key) {
            if previous == &input.draft {
                return Ok(());
            }
            if previous.revision >= input.draft.revision {
                return Err("草稿版本已变化，请重新加载".into());
            }
        }
        let mut data = r.data.clone();
        data.drafts.insert(key, input.draft.clone());
        data.drafts.insert(
            draft_key(
                &input.site_id,
                &input.account,
                &input.contest_id,
                &input.problem_key,
                "",
            ),
            input.draft,
        );
        persistence::write(&self.path, &data)?;
        r.data = data;
        Ok(())
    }

    pub(super) fn draft_for_language(
        &self,
        id: &str,
        cid: &str,
        pk: &str,
        language: &str,
    ) -> Result<Option<Draft>, String> {
        let r = self.runtime.lock().unwrap();
        let e = r.entries.get(id).ok_or("OJ 未配置")?;
        Ok(r.data
            .drafts
            .get(&draft_key(id, &e.username, cid, pk, language))
            .cloned())
    }
    pub(super) async fn submit(&self, input: SubmitInput) -> Result<Value, String> {
        self.submit_with_code(input, None).await
    }
    pub(super) async fn submit_with_code(
        &self,
        input: SubmitInput,
        source_code: Option<String>,
    ) -> Result<Value, String> {
        let id = &input.site_id;
        let cid = &input.contest_id;
        let e = self.entry(id)?;
        // Reject duplicate clicks rather than queuing a second POST behind the first.
        let _guard = e
            .gate
            .try_lock()
            .map_err(|_| "该 OJ 正在处理操作，请稍后查看结果")?;
        if e.epoch != input.epoch || e.username != input.account {
            return Err("账号已变化，请重新加载".into());
        }
        // Persist IDE code before any network request, including rejected or uncertain submissions.
        if let Some(code) = &source_code {
            let pk = input.problem_key.as_deref().ok_or("题目编号缺失")?;
            let mut draft = {
                let r = self.runtime.lock().unwrap();
                let preferences = Self::preferences_locked(&r, id, &e.username);
                let previous =
                    r.data
                        .drafts
                        .get(&draft_key(id, &e.username, cid, pk, &preferences.language));
                Draft {
                    code: code.clone(),
                    language: preferences.language.clone(),
                    options: preferences.options()?,
                    revision: (chrono::Utc::now().timestamp_millis() as u64)
                        .max(previous.map_or(0, |d| d.revision.saturating_add(1))),
                    ..Default::default()
                }
            };
            draft.answer = json!("");
            self.save_draft(DraftInput {
                site_id: id.clone(),
                contest_id: cid.clone(),
                problem_key: pk.into(),
                account: e.username.clone(),
                epoch: e.epoch,
                draft: draft.clone(),
            })?;
            if let Some(app) = &self.app {
                let _ = app.emit(
                    "oj-draft-changed",
                    json!({
                        "siteId": id, "contestId": cid, "problemKey": pk,
                        "account": e.username, "epoch": e.epoch, "draft": draft,
                    }),
                );
            }
        }
        let key = operation_key(id, &e.username, cid, input.problem_key.as_deref());
        if self
            .runtime
            .lock()
            .unwrap()
            .data
            .uncertain
            .contains_key(&key)
        {
            return Err("上次提交结果待确认，请先检查提交记录；确认后才能再次提交".into());
        }
        let paper = self.paper_locked(id, &e, cid).await?;
        if !paper.writable {
            return Err("当前测验不允许提交，请刷新查看状态".into());
        }
        let (path, method, body) = if let Some(pk) = &input.problem_key {
            let p = paper
                .problems
                .iter()
                .find(|p| problem_key(p) == *pk)
                .ok_or("题目不存在")?;
            if !matches!(p.kind, 0 | 4) {
                return Err("非编程题请提交答卷".into());
            }
            let mut draft = paper.drafts.get(pk).cloned().ok_or("请先保存代码")?;
            if let Some(source_code) = source_code {
                draft.code = source_code;
            }
            if draft.code.trim().is_empty() || !p.languages.contains(&draft.language) {
                return Err("请填写代码并选择题目允许的语言".into());
            }
            (
                if p.kind == 4 {
                    "code-fill-submission/"
                } else {
                    "code-submission/"
                }
                .into(),
                reqwest::Method::POST,
                {
                    let mut body = json!({"contest":cid.parse::<u64>().unwrap(),"problem":p.id.parse::<u64>().unwrap(),"language":draft.language,"code":draft.code});
                    if let Some(options) =
                        CompileOptions::for_language(&draft.language, draft.options.as_ref())?
                    {
                        body["options"] = options.wire();
                    }
                    body
                },
            )
        } else {
            let body = protocol::noncode_body(cid, &paper.problems, &paper.drafts)
                .map_err(|e| e.message)?;
            if let Some(submitted) = paper.submitted {
                let sid = protocol::id(&submitted["id"]).map_err(|e| e.message)?;
                (
                    format!("contest-submission/{sid}/"),
                    reqwest::Method::PATCH,
                    body,
                )
            } else {
                ("contest-submission/".into(), reqwest::Method::POST, body)
            }
        };
        // Journal before sending, including process death while waiting for the response.
        {
            let mut r = self.runtime.lock().unwrap();
            r.data.uncertain.insert(key.clone(),json!({"time":chrono::Utc::now().timestamp_millis(),"problemKey":input.problem_key}));
            self.persist(&r)?;
        }
        let result = e.client.request(method, &path, &[], Some(&body)).await;
        if !self.current(id, &e) {
            return Err("账号已变化，原提交请在对应账号记录中确认".into());
        }
        self.invalidate_progress(id, &e, cid);
        match result {
            Ok(value) => {
                if protocol::id(&value["id"]).is_err() {
                    return Err("提交响应缺少编号，结果待确认，请先查询提交记录".into());
                }
                let mut r = self.runtime.lock().unwrap();
                r.data.uncertain.remove(&key);
                self.persist(&r)?;
                Ok(value)
            }
            Err(error) => {
                if !error.uncertain {
                    let mut r = self.runtime.lock().unwrap();
                    r.data.uncertain.remove(&key);
                    self.persist(&r)?;
                }
                if error.status == Some(401) {
                    drop(_guard);
                    self.update_status(id, &e, Session::Disabled, "登录失效，草稿已保留");
                }
                Err(error.message)
            }
        }
    }
    pub(super) async fn records(
        &self,
        id: &str,
        cid: &str,
        kind: u8,
        pid: Option<&str>,
    ) -> Result<Value, String> {
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        protocol::id(&json!(cid)).map_err(|e| e.message)?;
        if let Some(pid) = pid {
            protocol::id(&json!(pid)).map_err(|e| e.message)?;
        }
        if kind > 4 {
            return Err("未知题型".into());
        }
        let path = match kind {
            0 => "code-submission/my_submissions/",
            4 => "code-fill-submission/my_submissions/",
            3 => "qa-submission/my_submissions/",
            _ => "contest-submission/",
        };
        let mut query = vec![("contest", cid.into())];
        if let Some(pid) = pid.filter(|_| matches!(kind, 0 | 3 | 4)) {
            query.push(("problem", pid.into()));
        }
        let mut records = e.client.list(path, &query).await.map_err(|e| e.message)?;
        if let Some(pid) = pid {
            if matches!(kind, 1 | 2) {
                let mut scoped = vec![];
                for row in records {
                    let sid = protocol::id(&row["id"]).map_err(|e| e.message)?;
                    let detail = self
                        .read(id, &e, &format!("contest-submission/{sid}/"), &[])
                        .await?;
                    if let Some(row) = protocol::record_for_problem(&detail, kind, pid) {
                        scoped.push(row);
                    }
                }
                records = scoped;
            } else {
                records = records
                    .iter()
                    .filter_map(|row| protocol::record_for_problem(row, kind, pid))
                    .collect();
            }
        }
        if !self.current(id, &e) {
            return Err("账号已变化，请重新加载".into());
        }
        let r = self.runtime.lock().unwrap();
        let uncertain: Vec<_> = r
            .data
            .uncertain
            .iter()
            .filter(|(key, _)| {
                serde_json::from_str::<(String, String, String, Option<String>)>(key).is_ok_and(
                    |(s, a, c, p)| {
                        s == id
                            && a == e.username
                            && c == cid
                            && pid.is_none_or(|pid| {
                                p.as_deref() == Some(&format!("{kind}:{pid}"))
                                    || (p.is_none() && matches!(kind, 1..=3))
                            })
                    },
                )
            })
            .map(|(key, value)| json!({"key":key,"details":value}))
            .collect();
        Ok(json!({"records":records,"uncertain":uncertain}))
    }
    pub(super) async fn result(
        &self,
        id: &str,
        sid: &str,
        kind: u8,
        pid: Option<&str>,
    ) -> Result<Value, String> {
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        protocol::id(&json!(sid)).map_err(|e| e.message)?;
        let prefix = match kind {
            4 => "code-fill-submission",
            3 => "qa-submission",
            1 | 2 => "contest-submission",
            _ => "code-submission",
        };
        let value = self.read(id, &e, &format!("{prefix}/{sid}/"), &[]).await?;
        if matches!(kind, 0 | 4) {
            if let Some(status) = value["status"].as_i64().filter(|s| *s != 6 && *s != 7) {
                let contest = value["contest"].get("id").unwrap_or(&value["contest"]);
                if let Ok(cid) = protocol::id(contest) {
                    let changed = {
                        let mut state = e.state.lock().unwrap();
                        let key = format!("{kind}:{sid}");
                        let changed = state.judged_progress.get(&key) != Some(&status);
                        if state.judged_progress.len() >= 512 {
                            state.judged_progress.clear();
                        }
                        state.judged_progress.insert(key, status);
                        changed
                    };
                    if changed {
                        self.invalidate_progress(id, &e, &cid);
                    }
                }
            }
        }
        if let Some(pid) = pid {
            protocol::record_for_problem(&value, kind, pid).ok_or("提交记录不属于当前题目".into())
        } else {
            Ok(value)
        }
    }
    pub(super) fn acknowledge(&self, id: &str, key: &str) -> Result<(), String> {
        let e = self.entry(id)?;
        let parts: (String, String, String, Option<String>) =
            serde_json::from_str(key).map_err(|_| "记录无效")?;
        if parts.0 != id || parts.1 != e.username {
            return Err("记录不属于当前账号".into());
        }
        let mut r = self.runtime.lock().unwrap();
        r.data.uncertain.remove(key);
        self.persist(&r)
    }
    pub(super) async fn send_companion(
        &self,
        id: &str,
        cid: &str,
        keys: Vec<String>,
        port: u16,
    ) -> Result<Value, String> {
        if cfg!(mobile) {
            return Err("手机端不提供 Competitive Companion".into());
        }
        if port == 0 {
            return Err("接收端口无效".into());
        }
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        let paper = self.paper_locked(id, &e, cid).await?;
        let site = self
            .runtime
            .lock()
            .unwrap()
            .data
            .sites
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or("OJ 不存在")?;
        let problems: Vec<_> = paper
            .problems
            .iter()
            .filter(|p| {
                matches!(p.kind, 0 | 4) && (keys.is_empty() || keys.contains(&problem_key(p)))
            })
            .collect();
        if problems.is_empty() {
            return Err("没有可发送的编程题".into());
        }
        if keys.is_empty() {
            let mut labels = std::collections::BTreeSet::new();
            if problems.iter().any(|p| !labels.insert(&p.display_label)) {
                return Err("不同题型包含相同题号，请分别发送，避免文件名冲突".into());
            }
        }
        let batch = uuid::Uuid::new_v4().to_string();
        let payloads = problems
            .iter()
            .map(|p| {
                protocol::companion(
                    &site,
                    &paper.contest,
                    p,
                    &batch,
                    problems.len(),
                    keys.is_empty(),
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.message)?;
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|_| "无法建立本机连接")?;
        let mut sent = 0;
        for payload in payloads {
            if e.client.cancel.is_cancelled() {
                return Err("操作已取消".into());
            }
            let result = http
                .post(format!("http://127.0.0.1:{port}/"))
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(&payload).unwrap())
                .send()
                .await;
            if !result.is_ok_and(|r| r.status().is_success()) {
                return Err(format!(
                    "本机工具接收失败（已发送 {sent}/{}），请检查工具端口；不会自动重发",
                    problems.len()
                ));
            }
            sent += 1;
        }
        let mut r = self.runtime.lock().unwrap();
        if r.data.companion_port != port {
            r.companion_status = CompanionStatus::default();
        }
        r.data.companion_port = port;
        self.persist(&r)?;
        drop(r);
        self.publish();
        Ok(json!({"sent":sent,"batch":batch}))
    }
}
impl Service {
    pub(super) async fn image(&self, id: &str, url: &str) -> Result<String, String> {
        let e = self.entry(id)?;
        let url = reqwest::Url::parse(url).map_err(|_| "题目图片地址无效")?;
        if url.origin() != e.client.base.origin()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("题目图片不属于当前 OJ".into());
        }
        let response = e
            .client
            .http
            .get(url)
            .header("Referer", e.client.base.as_str())
            .send();
        let mut response = tokio::select! {_=e.client.cancel.cancelled()=>return Err("账号已变化".into()),response=response=>response.map_err(|_|"图片加载失败")?};
        if !response.status().is_success() {
            return Err("图片不可访问".into());
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| "图片读取失败")? {
            if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
                return Err("图片超过 4 MB".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let format = image::guess_format(&bytes).map_err(|_| "仅支持普通图片格式")?;
        let mime = match format {
            image::ImageFormat::Png => "image/png",
            image::ImageFormat::Jpeg => "image/jpeg",
            image::ImageFormat::Gif => "image/gif",
            image::ImageFormat::WebP => "image/webp",
            _ => return Err("图片格式不支持".into()),
        };
        if !self.current(id, &e) {
            return Err("账号已变化".into());
        }
        use base64::Engine;
        Ok(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    }
}
