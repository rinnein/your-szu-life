use super::*;

/// 助教工作台的全部读取都走这里。注意：本模块不做任何缓存，也不写
/// `data.groups/contests/known/notices`，因此不会进入首页 todos、后台扫描或通知。
impl Service {
    pub async fn managed_contests(&self, id: &str) -> Result<ManagedContests, String> {
        self.review_permission(id)?;
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        self.review_permission(id)?;
        let rows = self.list(id, &e, "manage/contests/", &[]).await?;
        let now = self
            .server_now(id, &e)
            .await
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        let mut contests = Vec::with_capacity(rows.len());
        for row in rows {
            contests.push(protocol::managed_contest(&row).map_err(|error| error.message)?);
        }
        contests.sort_by(|a, b| b.begin.cmp(&a.begin).then_with(|| a.id.cmp(&b.id)));
        Ok(ManagedContests {
            rows: contests,
            now,
        })
    }

    pub async fn manage_contest(
        &self,
        id: &str,
        cid: &str,
    ) -> Result<ManagedContestDetail, String> {
        self.review_permission(id)?;
        protocol::id(&json!(cid)).map_err(|_| "测验编号无效")?;
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        self.review_permission(id)?;
        self.manage_contest_locked(id, &e, cid).await
    }

    async fn manage_contest_locked(
        &self,
        id: &str,
        e: &Entry,
        cid: &str,
    ) -> Result<ManagedContestDetail, String> {
        let raw = self
            .read(id, e, &format!("manage/contests/{cid}/"), &[])
            .await?;
        let contest = protocol::managed_contest(&raw).map_err(|error| error.message)?;
        let problems = protocol::problems(&raw).map_err(|error| error.message)?;
        let now = self
            .server_now(id, e)
            .await
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        Ok(ManagedContestDetail {
            contest,
            problems,
            now,
        })
    }

    pub async fn review_list(
        &self,
        id: &str,
        cid: &str,
        search: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<ReviewList, String> {
        self.review_permission(id)?;
        protocol::id(&json!(cid)).map_err(|_| "测验编号无效")?;
        let limit = limit.clamp(1, 100);
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        self.review_permission(id)?;
        let mut query = vec![
            ("contest", cid.to_string()),
            ("detail", "true".to_string()),
            ("ordering", "user__username".to_string()),
            ("offset", offset.to_string()),
            ("limit", limit.to_string()),
        ];
        if let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) {
            query.push(("search", search.to_string()));
        }
        let raw = self
            .read(id, &e, "manage/contest-submission/", &query)
            .await?;
        let (count, rows) = page(&raw)?;
        let mut list = Vec::with_capacity(rows.len());
        for row in rows {
            list.push(ReviewSummary {
                id: protocol::id(&row["id"]).map_err(|error| error.message)?,
                user: protocol::review_user(&row["user"]).map_err(|error| error.message)?,
                total_score: protocol::number_opt(&row["total_score"]).unwrap_or(0.),
                judged: row["judged"].as_bool().unwrap_or(false),
                checked: row["checked"].as_bool().unwrap_or(false),
            });
        }
        Ok(ReviewList { count, rows: list })
    }

    pub async fn review_detail(
        &self,
        id: &str,
        cid: &str,
        sid: &str,
    ) -> Result<ReviewDetail, String> {
        self.review_permission(id)?;
        protocol::id(&json!(cid)).map_err(|_| "测验编号无效")?;
        protocol::id(&json!(sid)).map_err(|_| "提交编号无效")?;
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        self.review_permission(id)?;
        self.review_detail_locked(id, &e, cid, sid).await
    }

    async fn review_detail_locked(
        &self,
        id: &str,
        e: &Entry,
        cid: &str,
        sid: &str,
    ) -> Result<ReviewDetail, String> {
        let contest_raw = self
            .read(id, e, &format!("manage/contests/{cid}/"), &[])
            .await?;
        let contest = protocol::managed_contest(&contest_raw).map_err(|error| error.message)?;
        let submission = self
            .read(id, e, &format!("manage/contest-submission/{sid}/"), &[])
            .await?;
        let user = protocol::review_user(&submission["user"]).map_err(|error| error.message)?;
        let total_score = protocol::number_opt(&submission["total_score"]).unwrap_or(0.);
        let judged = submission["judged"].as_bool().unwrap_or(false);
        let checked = submission["checked"].as_bool().unwrap_or(false);
        let problems = protocol::problems(&contest_raw).map_err(|error| error.message)?;
        let mut rows = Vec::with_capacity(problems.len());
        let mut warnings = vec![];
        for problem in &problems {
            let mut row = ReviewProblem {
                id: problem.id.clone(),
                display_label: problem.display_label.clone(),
                kind: problem.kind,
                title: problem.title.clone(),
                description: problem.description.clone(),
                hint: problem.hint.clone(),
                max_score: max_score(problem),
                score: 0.,
                submission_id: None,
                answer: Value::Null,
                choices: problem.choices.clone(),
                multiple: problem.multiple,
                blanks: problem.blanks,
                code: None,
                language: None,
                status: None,
                comment: String::new(),
                reference_answer: None,
                judge: None,
                load_error: None,
            };
            match problem.kind {
                0 | 4 => {
                    let map = if problem.kind == 0 {
                        &submission["coding_score"]
                    } else {
                        &submission["code_fill_score"]
                    };
                    if let Some((submission_id, score)) = first_score(&map[problem.id.as_str()]) {
                        row.submission_id = Some(submission_id.clone());
                        row.score = score;
                        let path = if problem.kind == 0 {
                            format!("code-submission/{submission_id}/")
                        } else {
                            format!("code-fill-submission/{submission_id}/")
                        };
                        match self.read(id, e, &path, &[]).await {
                            Ok(detail) => fill_code_submission(&mut row, &detail),
                            Err(error) => row.load_error = Some(error),
                        }
                    } else {
                        // 与平台一致：没有提交时仍以 -1 参与保存，分数为 0。
                        row.submission_id = Some("-1".into());
                    }
                }
                3 => {
                    if let Some((submission_id, score)) =
                        first_score(&submission["qa_score"][problem.id.as_str()])
                    {
                        row.submission_id = Some(submission_id.clone());
                        row.score = score;
                        match self
                            .read(
                                id,
                                e,
                                &format!("manage/qa-submission/{submission_id}/"),
                                &[],
                            )
                            .await
                        {
                            Ok(detail) => {
                                row.answer = detail["answer"].clone();
                                row.comment = protocol::text(&detail["comment"]);
                                row.judge = Some(detail);
                            }
                            Err(error) => row.load_error = Some(error),
                        }
                    } else {
                        row.submission_id = Some("-1".into());
                    }
                }
                1 | 2 => {
                    let field = if problem.kind == 1 {
                        "choice_submission"
                    } else {
                        "fill_in_submission"
                    };
                    row.answer = submission[field][problem.id.as_str()].clone();
                    let map = if problem.kind == 1 {
                        &submission["choice_score"]
                    } else {
                        &submission["fill_in_score"]
                    };
                    row.score = protocol::number_opt(&map[problem.id.as_str()]).unwrap_or(0.);
                }
                _ => {}
            }
            match self.reference_answer(id, e, problem).await {
                Ok(answer) => row.reference_answer = answer,
                Err(error) => warnings.push(format!(
                    "{} 参考答案读取失败：{error}",
                    problem.display_label
                )),
            }
            rows.push(row);
        }
        Ok(ReviewDetail {
            contest,
            submission_id: sid.to_string(),
            user,
            total_score,
            judged,
            checked,
            problems: rows,
            warnings,
        })
    }

    async fn reference_answer(
        &self,
        id: &str,
        e: &Entry,
        problem: &Problem,
    ) -> Result<Option<Value>, String> {
        let (path, field) = match problem.kind {
            0 => (format!("coding_problem/{}/", problem.id), "answer"),
            1 => (format!("manage/choices/{}/", problem.id), "solution"),
            2 => (
                format!("manage/fill-in-problems/{}/", problem.id),
                "solution",
            ),
            3 => (
                format!("manage/qa-problems/{}/", problem.id),
                "reference_answer",
            ),
            _ => return Ok(None),
        };
        let value = self.read(id, e, &path, &[]).await?;
        Ok(Some(value[field].clone()))
    }

    pub async fn save_review(&self, input: SaveReviewInput) -> Result<SaveReviewResult, String> {
        let id = input.site_id.as_str();
        self.review_permission(id)?;
        protocol::id(&json!(&input.contest_id)).map_err(|_| "测验编号无效")?;
        protocol::id(&json!(&input.submission_id)).map_err(|_| "提交编号无效")?;
        let e = self.entry(id)?;
        let result = {
            let _guard = e.gate.lock().await;
            self.ensure(id, &e).await?;
            self.review_permission(id)?;
            let contest_raw = self
                .read(id, &e, &format!("manage/contests/{}/", input.contest_id), &[])
                .await?;
            let problems = protocol::problems(&contest_raw).map_err(|error| error.message)?;
            validate_scores(&input.scores, &problems)?;
            let body = input.scores.wire();
            e.client
                .request(
                    reqwest::Method::PATCH,
                    &format!("manage/contest-submission/{}/", input.submission_id),
                    &[],
                    Some(&body),
                )
                .await
        };
        match result {
            Ok(_) => match self
                .review_detail(id, &input.contest_id, &input.submission_id)
                .await
            {
                Ok(detail) => Ok(SaveReviewResult {
                    saved: true,
                    uncertain: false,
                    message: String::new(),
                    detail: Some(detail),
                }),
                Err(error) => Ok(SaveReviewResult {
                    saved: true,
                    uncertain: false,
                    message: format!("保存已提交，但刷新失败：{error}"),
                    detail: None,
                }),
            },
            Err(error) => {
                if error.status == Some(401) {
                    self.update_status(id, &e, Session::Disabled, "登录已过期");
                }
                Ok(SaveReviewResult {
                    saved: false,
                    uncertain: error.uncertain,
                    message: error.display(),
                    detail: None,
                })
            }
        }
    }

    pub async fn save_review_comment(
        &self,
        input: ReviewCommentInput,
    ) -> Result<WriteResult, String> {
        let id = input.site_id.as_str();
        self.review_permission(id)?;
        protocol::id(&json!(&input.submission_id)).map_err(|_| "提交编号无效")?;
        if !matches!(input.kind, 0 | 3 | 4) {
            return Err("该题型不支持评语".into());
        }
        if input.comment.len() > 4000 {
            return Err("评语过长".into());
        }
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        self.review_permission(id)?;
        let prefix = if input.kind == 3 {
            "manage/qa-submission"
        } else {
            "manage/code-submission"
        };
        let body = json!({ "comment": input.comment });
        match e
            .client
            .request(
                reqwest::Method::PATCH,
                &format!("{prefix}/{}/", input.submission_id),
                &[],
                Some(&body),
            )
            .await
        {
            Ok(_) => Ok(WriteResult {
                saved: true,
                uncertain: false,
                message: String::new(),
            }),
            Err(error) => {
                if error.status == Some(401) {
                    self.update_status(id, &e, Session::Disabled, "登录已过期");
                }
                Ok(WriteResult {
                    saved: false,
                    uncertain: error.uncertain,
                    message: error.display(),
                })
            }
        }
    }

    pub async fn problem_submissions(
        &self,
        id: &str,
        cid: &str,
        pid: &str,
        kind: u8,
        search: Option<&str>,
        status: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<ProblemSubmissions, String> {
        self.review_permission(id)?;
        protocol::id(&json!(cid)).map_err(|_| "测验编号无效")?;
        protocol::id(&json!(pid)).map_err(|_| "题目编号无效")?;
        if !matches!(kind, 0 | 4) {
            return Err("仅支持编程题与代码补全题".into());
        }
        let limit = limit.clamp(1, 100);
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        self.review_permission(id)?;
        let path = if kind == 0 {
            "code-submission/"
        } else {
            "code-fill-submission/"
        };
        let mut query = vec![
            ("contest", cid.to_string()),
            ("problem", pid.to_string()),
            ("offset", offset.to_string()),
            ("limit", limit.to_string()),
        ];
        if let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) {
            query.push(("search", search.to_string()));
        }
        if let Some(status) = status.map(str::trim).filter(|value| !value.is_empty()) {
            query.push(("status", status.to_string()));
        }
        let raw = self.read(id, &e, path, &query).await?;
        let (count, rows) = page(&raw)?;
        let mut list = Vec::with_capacity(rows.len());
        for row in rows {
            list.push(ProblemSubmissionRow {
                id: protocol::id(&row["id"]).map_err(|error| error.message)?,
                user: protocol::review_user(&row["user"]).map_err(|error| error.message)?,
                language: protocol::text(&row["language"]),
                status: row["status"]
                    .as_i64()
                    .and_then(|value| i32::try_from(value).ok()),
                create_time: row
                    .get("create_time")
                    .or_else(|| row.get("time"))
                    .cloned()
                    .unwrap_or(Value::Null),
                cpu_time: protocol::number_opt(&row["cpu_time"])
                    .or_else(|| protocol::number_opt(&row["time_usage"])),
                real_time: protocol::number_opt(&row["real_time"]),
                memory: protocol::number_opt(&row["memory"])
                    .or_else(|| protocol::number_opt(&row["memory_usage"])),
                can_view_detail: row["can_view_detail"].as_bool().unwrap_or(true),
            });
        }
        Ok(ProblemSubmissions { count, rows: list })
    }

    pub async fn ta_submission(
        &self,
        id: &str,
        cid: &str,
        pid: &str,
        kind: u8,
        sid: &str,
    ) -> Result<TaSubmission, String> {
        self.review_permission(id)?;
        protocol::id(&json!(cid)).map_err(|_| "测验编号无效")?;
        protocol::id(&json!(pid)).map_err(|_| "题目编号无效")?;
        protocol::id(&json!(sid)).map_err(|_| "提交编号无效")?;
        if !matches!(kind, 0 | 4) {
            return Err("仅支持编程题与代码补全题".into());
        }
        let e = self.entry(id)?;
        let _guard = e.gate.lock().await;
        self.ensure(id, &e).await?;
        self.review_permission(id)?;
        let path = if kind == 0 {
            format!("code-submission/{sid}/")
        } else {
            format!("code-fill-submission/{sid}/")
        };
        let submission = self.read(id, &e, &path, &[]).await?;
        let mut warnings = vec![];
        let query = [("contest", cid.to_string())];
        let mut response = self
            .read(id, &e, &format!("coding_problem/{pid}/"), &query)
            .await;
        if response.is_err() && kind == 4 {
            response = self
                .read(id, &e, &format!("code_fill_problem/{pid}/"), &query)
                .await;
        }
        let (mut expected, samples) = match response {
            Ok(value) => (strings(&value["test_output"]), strings(&value["sample_output"])),
            Err(error) => {
                warnings.push(format!("期望输出读取失败：{error}"));
                (vec![], vec![])
            }
        };
        // 与平台 SubmissionDetails 对齐：竞赛开启 include_sample 时，result.data
        // 先包含样例，再包含隐藏测试点，期望输出也要按同样顺序拼接。
        if submission["contest"]["include_sample"].as_bool() == Some(true) {
            let mut combined = samples.clone();
            combined.append(&mut expected);
            expected = combined;
        }
        Ok(TaSubmission {
            submission,
            expected,
            samples,
            warnings,
        })
    }

    async fn server_now(&self, id: &str, e: &Entry) -> Option<i64> {
        match self.read(id, e, "info/time/", &[]).await {
            Ok(value) => protocol::timestamp_opt(&value["local_time"])
                .or_else(|| protocol::timestamp_opt(&value)),
            Err(error) => {
                log::warn!("[oj] 读取服务器时间失败: {error}");
                None
            }
        }
    }
}

fn page(value: &Value) -> Result<(u64, &[Value]), String> {
    if let Some(rows) = value.as_array() {
        return Ok((rows.len() as u64, rows));
    }
    let count = value["count"]
        .as_u64()
        .ok_or_else(|| "OJ 分页总数缺失".to_string())?;
    let rows = value["results"]
        .as_array()
        .ok_or_else(|| "OJ 列表格式不兼容".to_string())?;
    Ok((count, rows))
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn max_score(problem: &Problem) -> f64 {
    problem.score.unwrap_or(if matches!(problem.kind, 0 | 3 | 4) {
        100.
    } else {
        0.
    })
}

/// 分数表形如 `{problemId: {submissionId: score}}`；平台只取一个提交，
/// 这里优先选择编号最大的提交，避免 JSON 键顺序变化导致拿错历史提交。
fn first_score(value: &Value) -> Option<(String, f64)> {
    let rows = value.as_object()?;
    let mut best: Option<(String, f64)> = None;
    for (submission_id, raw) in rows {
        let Some(score) = protocol::number_opt(raw) else {
            continue;
        };
        let rank = submission_id.parse::<i64>().unwrap_or(-1);
        let better = best
            .as_ref()
            .is_none_or(|(current, _)| rank > current.parse::<i64>().unwrap_or(-1));
        if better {
            best = Some((submission_id.clone(), score));
        }
    }
    best
}

fn fill_code_submission(row: &mut ReviewProblem, detail: &Value) {
    row.code = detail["code"].as_str().map(str::to_owned);
    row.language = detail["language"].as_str().map(str::to_owned);
    row.status = detail["status"]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok());
    row.comment = protocol::text(&detail["comment"]);
    row.judge = Some(detail.clone());
}

fn validate_scores(scores: &ReviewScores, problems: &[Problem]) -> Result<(), String> {
    let mut meta = BTreeMap::new();
    for problem in problems {
        meta.insert(problem.id.as_str(), problem);
    }
    for (name, rows, kind) in [
        ("编程题", &scores.coding, 0u8),
        ("代码补全题", &scores.code_fill, 4u8),
        ("问答题", &scores.qa, 3u8),
    ] {
        for (pid, submissions) in rows {
            let problem = meta
                .get(pid.as_str())
                .ok_or_else(|| format!("{name}题目 {pid} 不属于该测验"))?;
            if problem.kind != kind {
                return Err(format!("{name}题目 {pid} 的题型不匹配"));
            }
            let max = max_score(problem);
            for (submission_id, score) in submissions {
                if submission_id != "-1" {
                    protocol::id(&json!(submission_id)).map_err(|_| "提交编号无效")?;
                }
                check_score(*score, max, &problem.display_label)?;
            }
        }
    }
    for (name, rows, kind) in [
        ("选择题", &scores.choice, 1u8),
        ("填空题", &scores.fill_in, 2u8),
    ] {
        for (pid, score) in rows {
            let problem = meta
                .get(pid.as_str())
                .ok_or_else(|| format!("{name}题目 {pid} 不属于该测验"))?;
            if problem.kind != kind {
                return Err(format!("{name}题目 {pid} 的题型不匹配"));
            }
            check_score(*score, max_score(problem), &problem.display_label)?;
        }
    }
    Ok(())
}

fn check_score(score: f64, max: f64, label: &str) -> Result<(), String> {
    if !score.is_finite() || score < 0. || score > max {
        return Err(format!("{label} 的分数必须在 0 到 {max} 之间"));
    }
    Ok(())
}
