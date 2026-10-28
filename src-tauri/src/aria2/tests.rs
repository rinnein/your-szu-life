use super::{
    binary::find_aria2c_binary,
    client::Aria2WebSocketClient,
    downloader::{verify_file_hash_and_size, PROGRESS_POLL_INTERVAL_MS},
    process::{base_args, proxy_injection, Aria2Lease},
};
use crate::netproxy::Aria2Proxy;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

#[test]
fn base_args_keep_the_expected_rpc_and_transfer_options() {
    let args = base_args(54001, "secret", Path::new("/tmp/aria2c.log"));
    for expected in [
        "--enable-rpc=true",
        "--rpc-listen-all=false",
        "--rpc-listen-port=54001",
        "--rpc-secret=secret",
        "--split=16",
        "--no-conf=true",
        "--check-certificate=false",
        "--log=/tmp/aria2c.log",
    ] {
        assert!(args.iter().any(|arg| arg == expected), "缺少 {expected}");
    }
    // --no-conf 必须在：只有这一份参数表，没有配置文件会覆盖它。
    assert!(args.iter().all(|arg| !arg.starts_with("--all-proxy")));
}

#[test]
fn disabled_proxy_clears_every_proxy_env_var() {
    let injection = proxy_injection(&Aria2Proxy::Disabled);
    assert_eq!(injection.args, vec!["--all-proxy=".to_owned()]);
    for key in [
        "http_proxy",
        "HTTP_PROXY",
        "https_proxy",
        "HTTPS_PROXY",
        "all_proxy",
        "ALL_PROXY",
        "ftp_proxy",
        "FTP_PROXY",
    ] {
        assert!(injection.removed_env.contains(&key), "未清理 {key}");
    }
}

#[test]
fn auto_proxy_leaves_aria2_alone() {
    let injection = proxy_injection(&Aria2Proxy::Environment);
    assert!(injection.args.is_empty());
    assert!(injection.removed_env.is_empty());
}

#[test]
fn manual_proxy_wins_over_the_environment() {
    let injection = proxy_injection(&Aria2Proxy::Manual(
        "http://user:pass@proxy.example:7890".into(),
    ));
    assert_eq!(
        injection.args,
        vec!["--all-proxy=http://user:pass@proxy.example:7890".to_owned()]
    );
    assert!(!injection.removed_env.is_empty());
}

#[test]
fn test_find_aria2c_binary() {
    let result = find_aria2c_binary(None);
    assert!(
        result.is_ok(),
        "find_aria2c_binary should succeed on system or dev build: {:?}",
        result.err()
    );
    let path = result.unwrap();
    assert!(path.is_file(), "Binary path {:?} must exist", path);
}

#[tokio::test]
async fn test_aria2_lease_lifecycle_rpc_and_reuse() {
    let lease1 = Aria2Lease::acquire(None)
        .await
        .expect("lease1 should be acquired");
    let ws_url1 = lease1.ws_url.clone();
    let secret1 = lease1.secret.clone();
    let port1 = lease1.port;
    assert!(port1 >= crate::aria2::process::DEFAULT_ARIA2_PORT);

    let mut client = Aria2WebSocketClient::connect(&lease1.ws_url, &lease1.secret)
        .await
        .expect("failed to connect via websocket");

    let version_val = client
        .call_raw("aria2.getVersion", (format!("token:{}", lease1.secret),))
        .await
        .expect("failed to call aria2.getVersion");

    assert!(
        version_val.get("version").is_some(),
        "Version field must be present in getVersion response: {:?}",
        version_val
    );

    let uri_gid = client
        .add_uri(
            &["https://example.com/sample2.txt".to_string()],
            std::path::Path::new("/tmp"),
            "sample2.txt",
        )
        .await
        .expect("add_uri should return GID");
    assert!(!uri_gid.is_empty(), "add_uri should return a non-empty GID");

    // Concurrent second task should reuse the exact same running instance and port
    let lease2 = Aria2Lease::acquire(None)
        .await
        .expect("lease2 should be acquired");
    assert_eq!(lease2.ws_url, ws_url1);
    assert_eq!(lease2.secret, secret1);
    assert_eq!(lease2.port, port1);

    // Release first task, second is still active
    lease1.release().await;

    // Process must still be alive for lease2
    assert!(lease2.check_alive().await.is_ok());

    // Release second task, ref_count reaches 0 -> process shuts down
    lease2.release().await;
}

#[test]
fn progress_poll_interval_stays_in_reasonable_band() {
    // 进度只能靠轮询 tellStatus 获取（aria2 1.37 无 setNotification，
    // onDownloadProgress 默认不广播）。这里锁定一个既能驱动平滑动画、
    // 又不会让 RPC 往返过于频繁的区间，避免有人把它改成 0 或几秒。
    let interval = PROGRESS_POLL_INTERVAL_MS;
    assert!(
        (50..=500).contains(&interval),
        "PROGRESS_POLL_INTERVAL_MS={interval} 超出合理区间 50..=500ms"
    );
}

/// aria2 1.37 已移除按 gid 订阅通知的 `aria2.setNotification`，
/// 该测试把这一事实固化下来：若它开始返回成功，说明升级到了支持
/// 推送进度的版本，届时可以重新评估是否去掉轮询。
#[tokio::test]
async fn aria2_set_notification_is_unavailable_so_progress_must_poll() {
    let lease = Aria2Lease::acquire(None)
        .await
        .expect("lease should be acquired");
    let mut client = Aria2WebSocketClient::connect(&lease.ws_url, &lease.secret)
        .await
        .expect("failed to connect via websocket");

    let result = client
        .call_raw(
            "aria2.setNotification",
            (
                format!("token:{}", lease.secret),
                "gidplaceholder".to_string(),
                vec!["aria2.onDownloadProgress"],
            ),
        )
        .await;

    let err = result.expect_err("aria2.setNotification should not be supported in 1.37");
    assert!(
        err.contains("No such method"),
        "预期 setNotification 不存在，实际错误: {err}"
    );

    lease.release().await;
}

#[test]
fn test_verify_file_hash_and_size() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"hello aria2").unwrap();
    tmp.flush().unwrap();

    // sha256 of "hello aria2" is eebfafaa520b6266a21a5ecb4fa108ad86dd06bf800b729460589198837c23ce
    let hash = "eebfafaa520b6266a21a5ecb4fa108ad86dd06bf800b729460589198837c23ce";
    assert!(verify_file_hash_and_size(tmp.path(), Some(11), Some(hash)));
    assert!(!verify_file_hash_and_size(tmp.path(), Some(10), Some(hash)));
    assert!(!verify_file_hash_and_size(
        tmp.path(),
        Some(11),
        Some("wrong_hash")
    ));
}
