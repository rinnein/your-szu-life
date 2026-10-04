use super::*;
impl Service {
    pub(super) fn preferences_locked(r: &Runtime, id: &str, account: &str) -> Preferences {
        if let Some(saved) = r.data.preferences.get(id) {
            return saved.clone();
        }
        // Migrate the most recently edited code draft without changing its source.
        let previous = r
            .data
            .drafts
            .iter()
            .filter(|(key, d)| {
                !d.language.is_empty()
                    && serde_json::from_str::<(String, String, String, String, String)>(key)
                        .is_ok_and(|(site, user, _, problem, _)| {
                            site == id
                                && user == account
                                && (problem.starts_with("0:") || problem.starts_with("4:"))
                        })
            })
            .max_by_key(|(_, draft)| draft.revision)
            .map(|(_, d)| d);
        let mut value = Preferences::default();
        if let Some(draft) = previous {
            value.language = draft.language.clone();
            if let Some(options) = &draft.options {
                value
                    .options_by_language
                    .insert(value.language.clone(), options.clone());
            }
        }
        value
    }
    pub(super) fn save_preferences(&self, input: PreferencesInput) -> Result<Preferences, String> {
        if !["c", "cpp", "java", "py", "go", "js"].contains(&input.language.as_str()) {
            return Err("不支持的编程语言".into());
        }
        let options = CompileOptions::for_language(&input.language, input.options.as_ref())?;
        let mut r = self.runtime.lock().unwrap();
        let e = r.entries.get(&input.site_id).ok_or("OJ 未配置")?;
        if e.epoch != input.epoch || e.username != input.account {
            return Err("账号已变化，请重新打开提交窗口".into());
        }
        let mut preferences = Self::preferences_locked(&r, &input.site_id, &input.account);
        if preferences.revision != input.expected_revision {
            return Err("编译配置已在另一窗口更新，请刷新".into());
        }
        preferences.language = input.language;
        if let Some(options) = options {
            preferences
                .options_by_language
                .insert(preferences.language.clone(), options);
        }
        preferences.revision += 1;
        let mut data = r.data.clone();
        data.preferences
            .insert(input.site_id.clone(), preferences.clone());
        persistence::write(&self.path, &data)?;
        r.data = data;
        drop(r);
        if let Some(app) = &self.app {
            let _ = app.emit(
                "oj-preferences-changed",
                json!({"siteId":input.site_id,"preferences":preferences}),
            );
        }
        Ok(preferences)
    }
}
