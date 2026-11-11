use super::{
    http::{HttpError, HttpResponse, PortalHttp},
    network::{NetworkEnvironment, NetworkSnapshot},
};
use reqwest::Url;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

pub const TEST_IP: &str = "172.28.0.8";
pub struct FakeHttp<F>(pub F);
impl<F: Fn(&Url) -> Result<HttpResponse, HttpError> + Sync> PortalHttp for FakeHttp<F> {
    fn get(&self, url: &Url) -> Result<HttpResponse, HttpError> {
        self.0(url)
    }
}
pub fn html(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        content_type: Some("text/html; charset=utf-8".into()),
        body: body.as_bytes().to_vec(),
        ..Default::default()
    }
}
pub fn jsonp(url: &Url, value: Value) -> HttpResponse {
    let callback = url
        .query_pairs()
        .find(|(key, _)| key == "callback")
        .unwrap()
        .1;
    html(&format!("{callback}({value});"))
}
pub fn staff_html(ip: &str, mac_auth: bool) -> String {
    format!(
        r#"<script>var CONFIG = {{ page: 'account', acid: "2", ip: '{ip}', lang: "zh-CN" || 'zh-CN', portal: {{"MacAuth":{mac_auth},"ServiceIP":"https://net.szu.edu.cn:8800"}} }};</script>"#
    )
}
pub fn staff_online(ip: &str) -> Value {
    json!({"error":"ok","user_name":"001234","real_name":"测试同学","online_ip":ip,
        "online_ip6":"fd7a:115c:a1e0::1234", "user_mac":"0000-0000-0001", "domain":"",
        "sum_bytes":25473392014_u64, "sum_seconds":663767, "add_time":1788827879_u64,
        "online_device_detail":"{\"other-device\":{\"ip\":\"172.28.0.99\"}}"})
}
pub fn dorm_html(ip: &str, online: bool) -> String {
    format!("<!--Dr.COMWebLoginID_{}.htm--><script>v4ip='{ip}';uid='001234';NID='宿舍同学';olmac='000000000001';vlanid='1';</script>", if online { 1 } else { 0 })
}
pub fn snapshot() -> NetworkSnapshot {
    NetworkSnapshot {
        connected: true,
        ip: Some(TEST_IP.into()),
        ssid: Some("shared-campus-network".into()),
        ..Default::default()
    }
}
pub struct FakeEnvironment {
    pub current: Mutex<NetworkSnapshot>,
    pub now: AtomicU64,
    pub sleeps: Mutex<Vec<Duration>>,
}
impl Default for FakeEnvironment {
    fn default() -> Self {
        Self {
            current: Mutex::new(snapshot()),
            now: AtomicU64::new(1788828000000),
            sleeps: Mutex::new(Vec::new()),
        }
    }
}
impl NetworkEnvironment for FakeEnvironment {
    fn snapshot(&self) -> NetworkSnapshot {
        self.current.lock().unwrap().clone()
    }
    fn now_ms(&self) -> u64 {
        self.now.fetch_add(1, Ordering::Relaxed)
    }
    fn sleep(&self, duration: Duration) {
        self.sleeps.lock().unwrap().push(duration);
        self.now
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
    }
}
