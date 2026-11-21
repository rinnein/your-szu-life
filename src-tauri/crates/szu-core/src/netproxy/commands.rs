use super::{
    model::{ProxyProbeView, ProxySettingsInput, ProxySettingsView},
    ProxyStore,
};
use reqwest::Url;
use tauri::State;

#[tauri::command]
pub fn get_proxy_settings(
    store: State<'_, ProxyStore>,
) -> Result<ProxySettingsView, String> {
    super::model::view(store.credentials.as_ref())
}

#[tauri::command]
pub fn save_proxy_settings(
    app: tauri::AppHandle,
    settings: ProxySettingsInput,
    store: State<'_, ProxyStore>,
) -> Result<ProxySettingsView, String> {
    super::save(store.credentials.as_ref(), &app, settings)
}

/// 用**待保存**的设置试连一次，好让用户在落盘前就知道代理是否可用。
#[tauri::command]
pub async fn test_proxy_settings(
    settings: ProxySettingsInput,
    url: Option<String>,
    store: State<'_, ProxyStore>,
) -> Result<ProxyProbeView, String> {
    let credentials = store.credentials.clone();
    tauri::async_runtime::spawn_blocking(move || {
        super::test(credentials.as_ref(), settings, url.as_deref())
    })
    .await
    .map_err(|_| "代理连通性测试任务异常结束".to_owned())?
}

/// 批量判定 URL 是否属于校内资源（campus-network.json 白名单）。
///
/// 供前端在导出/复制内容时决定「来源行是否省略、内网图片是否内联」；
/// 解析失败的 URL 按公网处理，调用方据此保留原行为。
#[tauri::command]
pub fn check_campus_targets(urls: Vec<String>) -> Vec<bool> {
    urls.iter()
        .map(|raw| {
            Url::parse(raw)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .is_some_and(|host| super::model::is_campus_target(&host))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::check_campus_targets;

    #[test]
    fn classifies_campus_and_public_targets() {
        let results = check_campus_targets(vec![
            "http://172.31.221.67/images/a.png".into(),
            "http://net.szu.edu.cn/".into(),
            "http://oj.szu.edu.cn/problem/1".into(),
            "https://github.com/a.png".into(),
            "http://8.8.8.8/a.png".into(),
            "not a url".into(),
        ]);
        assert_eq!(results, vec![true, true, true, false, false, false]);
    }
}