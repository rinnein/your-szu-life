use super::{
    binary::find_aria2c_binary,
    client::Aria2WebSocketClient,
    downloader::verify_file_hash_and_size,
    process::Aria2Lease,
};
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_find_aria2c_binary() {
    let result = find_aria2c_binary(None);
    assert!(result.is_ok(), "find_aria2c_binary should succeed on system or dev build: {:?}", result.err());
    let path = result.unwrap();
    assert!(path.is_file(), "Binary path {:?} must exist", path);
}

#[tokio::test]
async fn test_aria2_process_and_websocket_rpc() {
    let lease = Aria2Lease::acquire(None)
        .await
        .expect("failed to acquire lease");

    let mut client = Aria2WebSocketClient::connect(&lease.ws_url, &lease.secret)
        .await
        .expect("failed to connect via websocket");

    let version_val = client
        .call_raw("aria2.getVersion", (format!("token:{}", lease.secret),))
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
    assert!(!verify_file_hash_and_size(tmp.path(), Some(11), Some("wrong_hash")));
}

#[tokio::test]
async fn test_aria2_lease_reuse_and_shutdown() {
    let lease1 = Aria2Lease::acquire(None)
        .await
        .expect("lease1 should be acquired");
    let ws_url1 = lease1.ws_url.clone();
    let secret1 = lease1.secret.clone();

    // Concurrent second task should reuse the exact same running instance and port
    let lease2 = Aria2Lease::acquire(None)
        .await
        .expect("lease2 should be acquired");
    assert_eq!(lease2.ws_url, ws_url1);
    assert_eq!(lease2.secret, secret1);

    // Release first task, second is still active
    lease1.release().await;

    // Process must still be alive for lease2
    assert!(lease2.check_alive().await.is_ok());

    // Release second task, ref_count reaches 0 -> process shuts down
    lease2.release().await;
}
