use super::{
    binary::find_aria2c_binary,
    client::Aria2WebSocketClient,
    downloader::{create_metalink_xml, verify_file_hash_and_size},
    process::Aria2Process,
};
use base64::Engine;
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
    let binary = find_aria2c_binary(None).expect("failed to locate aria2c");
    let mut process = Aria2Process::spawn(&binary).expect("failed to spawn aria2c");

    let mut client = Aria2WebSocketClient::connect(&process.ws_url(), &process.secret)
        .await
        .expect("failed to connect via websocket");

    let version_val = client
        .call_raw("aria2.getVersion", (format!("token:{}", process.secret),))
        .await
        .expect("failed to call aria2.getVersion");

    assert!(
        version_val.get("version").is_some(),
        "Version field must be present in getVersion response: {:?}",
        version_val
    );

    let metalink_xml = create_metalink_xml(
        "sample.txt",
        &["https://example.com/sample.txt".to_string()],
        None,
        None,
    );
    let b64 = base64::engine::general_purpose::STANDARD.encode(metalink_xml.as_bytes());
    let gid = client
        .add_metalink(&b64, std::path::Path::new("/tmp"))
        .await
        .expect("add_metalink should return GID");
    assert!(!gid.is_empty(), "add_metalink should return a non-empty GID");

    let _ = client.shutdown().await;
    process.kill();
}

#[test]
fn test_create_metalink_xml() {
    let uris = vec![
        "https://example.com/model.onnx".to_string(),
        "https://mirror1.com/model.onnx".to_string(),
        "https://mirror2.com/model.onnx?a=1&b=2".to_string(),
    ];
    let xml = create_metalink_xml(
        "model.onnx",
        &uris,
        Some(100),
        Some("abcdef1234567890"),
    );

    assert!(xml.contains("<file name=\"model.onnx\">"));
    assert!(xml.contains("<size>100</size>"));
    assert!(xml.contains("<hash type=\"sha-256\">abcdef1234567890</hash>"));
    assert!(xml.contains("<url priority=\"1\">https://example.com/model.onnx</url>"));
    assert!(xml.contains("<url priority=\"2\">https://mirror1.com/model.onnx</url>"));
    assert!(xml.contains("<url priority=\"2\">https://mirror2.com/model.onnx?a=1&amp;b=2</url>"));
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
