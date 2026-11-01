//! 播放器媒体地址工具：m3u8 重写与 `jxxk-media:` 地址编解码。
//!
//! 播放链路：Rust 把 m3u8 内的分片、EXT-X-MAP 与 EXT-X-KEY 的 URI 重写为
//! `jxxk-media:<base64url(目标 https URL)>`；前端 hls.js 的自定义 loader
//! （src/patrol/mediaLoader.ts）解码后经 Tauri IPC（`jxxk_fetch_media`）回源
//! 拉取字节，完全避开 WebView 对自定义协议 XHR 的平台差异。

use base64::Engine;
use reqwest::Url;

/// 播放器 hls.js 自定义 loader 的媒体地址前缀：`jxxk-media:<base64url(目标)>`。
pub(crate) const MEDIA_SCHEME: &str = "jxxk-media:";

/// 把巡课域的绝对 https URL 编码为播放器媒体 URL。
pub(crate) fn media_url(target: &str) -> String {
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(target.as_bytes());
    format!("{MEDIA_SCHEME}{encoded}")
}

/// 解码播放器媒体 URL 中的目标地址。
pub(crate) fn decode_media_url(raw: &str) -> Result<Url, String> {
    let encoded = raw
        .strip_prefix(MEDIA_SCHEME)
        .ok_or_else(|| "媒体地址缺少前缀".to_string())?
        .trim_start_matches("//");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "媒体地址编码无效".to_string())?;
    let text = String::from_utf8(bytes).map_err(|_| "媒体地址编码无效".to_string())?;
    Url::parse(&text).map_err(|_| "媒体目标地址无效".to_string())
}

/// 校验目标必须在深大域内（防止把应用内代理当作任意代理使用）。
pub(crate) fn trusted_target(url: &Url) -> bool {
    let Some(host) = url.host_str().map(|h| h.trim_end_matches('.')) else {
        return false;
    };
    url.scheme() == "https" && (host == "szu.edu.cn" || host.ends_with(".szu.edu.cn"))
}

/// 重写 m3u8 中的 URI 引用（分片行、EXT-X-KEY 与 EXT-X-MAP 的 URI 属性）。
pub(crate) fn rewrite_m3u8(base: &Url, body: &str) -> String {
    let mut output = Vec::with_capacity(body.lines().count());
    for line in body.lines() {
        let rewritten = if let Some(uri) = attribute_uri(line) {
            match rewrite_uri(base, uri) {
                Ok(new) => line.replace(&format!("URI=\"{uri}\""), &format!("URI=\"{new}\"")),
                Err(_) => line.to_owned(),
            }
        } else if !line.starts_with('#') && !line.trim().is_empty() {
            match rewrite_uri(base, line.trim()) {
                Ok(new) => new,
                Err(_) => line.to_owned(),
            }
        } else {
            line.to_owned()
        };
        output.push(rewritten);
    }
    let mut text = output.join("\n");
    if body.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// 取行内第一个 `URI="..."` 属性值。
fn attribute_uri(line: &str) -> Option<&str> {
    let start = line.find("URI=\"")? + 5;
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn rewrite_uri(base: &Url, uri: &str) -> Result<String, String> {
    let resolved = base
        .join(uri)
        .map_err(|_| "m3u8 内相对地址无效".to_string())?;
    if !trusted_target(&resolved) {
        return Err("m3u8 内地址不受信任".into());
    }
    Ok(media_url(resolved.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_url_roundtrip() {
        let target = "https://jxxk.szu.edu.cn/vv/abc/group1/M00/0A/0A/x.mp4/index.m3u8?t=1&sign=s";
        let encoded = media_url(target);
        assert!(encoded.starts_with("jxxk-media:"));
        let decoded = decode_media_url(&encoded).unwrap();
        assert_eq!(decoded.as_str(), target);
    }

    #[test]
    fn untrusted_targets_are_rejected() {
        assert!(!trusted_target(
            &"https://evil.example.com/x".parse().unwrap()
        ));
        assert!(!trusted_target(
            &"http://jxxk.szu.edu.cn/x".parse().unwrap()
        ));
        assert!(!trusted_target(
            &"https://szu.edu.cn.evil.com/x".parse().unwrap()
        ));
        assert!(trusted_target(
            &"https://jxxk.szu.edu.cn/x".parse().unwrap()
        ));
        assert!(trusted_target(
            &"https://media-service.szu.edu.cn/x".parse().unwrap()
        ));
    }

    #[test]
    fn m3u8_uris_are_rewritten_to_media_scheme() {
        let base: Url = "https://jxxk.szu.edu.cn/vv/abc/x.mp4/index.m3u8?t=1&sign=s"
            .parse()
            .unwrap();
        let body = "#EXTM3U\n\
            #EXT-X-KEY:METHOD=AES-128,URI=\"/media-service/public/video/decryptKey?id=x&t=1&sign=s\",IV=0xD1\n\
            #EXT-X-MAP:URI=\"init-v1-a1.mp4\"\n\
            #EXTINF:8.0,\n\
            seg-1-v1-a1.m4s\n";
        let rewritten = rewrite_m3u8(&base, body);
        assert!(rewritten.contains("jxxk-media:"));
        assert!(!rewritten.contains("/media-service/public/video/decryptKey"));
        assert!(!rewritten.contains("\nseg-1-v1-a1.m4s"));
        // 每个被重写的地址解码后应还原为绝对源地址。
        for line in rewritten.lines() {
            if let Some(uri) = attribute_uri(line) {
                assert!(uri.starts_with("jxxk-media:"));
                let decoded = decode_media_url(uri).unwrap();
                assert!(decoded.as_str().starts_with("https://jxxk.szu.edu.cn/"));
            }
        }
    }
}
