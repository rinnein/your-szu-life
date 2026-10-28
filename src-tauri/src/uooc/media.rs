//! Metadata only: no cookie client, no decoder, and no complete large video download.
use reqwest::{
    blocking::{Client, Response},
    header::{CONTENT_RANGE, RANGE},
    Url,
};
use std::{
    collections::HashSet,
    io::{self, Read, Seek, SeekFrom},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const BLOCK: u64 = 64 * 1024;
const LIMIT: usize = 16 * 1024 * 1024;

fn public_url(url: &Url) -> bool {
    matches!(url.scheme(), "https" | "http")
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| {
            host != "localhost"
                && !host.ends_with(".localhost")
                && !host.ends_with(".local")
                && match host.trim_matches(['[', ']']).parse::<std::net::IpAddr>() {
                    Ok(std::net::IpAddr::V4(ip)) => {
                        !ip.is_private()
                            && !ip.is_loopback()
                            && !ip.is_link_local()
                            && !ip.is_unspecified()
                    }
                    Ok(std::net::IpAddr::V6(ip)) => {
                        !ip.is_loopback()
                            && !ip.is_unspecified()
                            && (ip.segments()[0] & 0xfe00) != 0xfc00
                            && (ip.segments()[0] & 0xffc0) != 0xfe80
                    }
                    _ => true,
                }
        })
}
fn io_error() -> io::Error {
    io::Error::other("视频元数据读取失败")
}
fn read_body(response: Response, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = vec![];
    response
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "读取视频元数据失败")?;
    if bytes.len() > limit {
        return Err("视频元数据超过读取上限".into());
    }
    Ok(bytes)
}
fn range(response: &Response) -> Option<(u64, u64, u64)> {
    let header = response
        .headers()
        .get(CONTENT_RANGE)?
        .to_str()
        .ok()?
        .strip_prefix("bytes ")?;
    let (part, size) = header.split_once('/')?;
    let (start, end) = part.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()?, size.parse().ok()?))
}

struct RemoteReader {
    client: Client,
    url: Url,
    cancel: CancellationToken,
    len: u64,
    pos: u64,
    start: u64,
    bytes: Vec<u8>,
    spent: usize,
    complete: bool,
}
impl RemoteReader {
    fn new(client: Client, url: Url, cancel: CancellationToken) -> Result<Self, String> {
        if cancel.is_cancelled() {
            return Err("已取消".into());
        }
        let response = client
            .get(url.clone())
            .header(RANGE, format!("bytes=0-{}", BLOCK - 1))
            .send()
            .map_err(|_| "无法读取视频元数据")?;
        let partial = response.status().as_u16() == 206;
        if !partial && response.status().as_u16() != 200 {
            return Err("视频服务器拒绝读取".into());
        }
        let length = if partial {
            let (start, end, total) = range(&response).ok_or("视频服务器返回了无效 Range")?;
            if start != 0 || end >= total || end >= BLOCK {
                return Err("视频 Range 不匹配".into());
            }
            Some((end + 1, total))
        } else {
            None
        };
        let bytes = read_body(response, if partial { BLOCK as usize } else { LIMIT })?;
        if length.is_some_and(|(expected, _)| expected != bytes.len() as u64) {
            return Err("视频 Range 长度不匹配".into());
        }
        let len = length.map(|(_, total)| total).unwrap_or(bytes.len() as u64);
        Ok(Self {
            client,
            url,
            cancel,
            len,
            pos: 0,
            start: 0,
            spent: bytes.len(),
            bytes,
            complete: !partial,
        })
    }
}
impl Read for RemoteReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(io_error());
        }
        if self.pos >= self.len || output.is_empty() {
            return Ok(0);
        }
        if self.pos < self.start || self.pos >= self.start + self.bytes.len() as u64 {
            if self.complete || self.spent >= LIMIT {
                return Err(io_error());
            }
            let end = (self.pos + BLOCK - 1).min(self.len - 1);
            let response = self
                .client
                .get(self.url.clone())
                .header(RANGE, format!("bytes={}-{end}", self.pos))
                .send()
                .map_err(|_| io_error())?;
            if response.status().as_u16() != 206
                || range(&response) != Some((self.pos, end, self.len))
            {
                return Err(io_error());
            }
            self.bytes =
                read_body(response, (end - self.pos + 1) as usize).map_err(|_| io_error())?;
            if self.bytes.len() as u64 != end - self.pos + 1 {
                return Err(io_error());
            }
            self.spent += self.bytes.len();
            self.start = self.pos;
        }
        let offset = (self.pos - self.start) as usize;
        let count = output.len().min(self.bytes.len() - offset);
        output[..count].copy_from_slice(&self.bytes[offset..offset + count]);
        self.pos += count as u64;
        Ok(count)
    }
}
impl Seek for RemoteReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let next = match from {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.pos as i128 + n as i128,
            SeekFrom::End(n) => self.len as i128 + n as i128,
        };
        if next < 0 || next > u64::MAX as i128 {
            return Err(io_error());
        }
        self.pos = next as u64;
        Ok(self.pos)
    }
}

#[derive(Debug, PartialEq)]
enum PlaylistStep {
    Duration(f64),
    Variant(String),
}
fn parse_hls(bytes: &[u8]) -> Result<PlaylistStep, String> {
    match m3u8_rs::parse_playlist_res(bytes).map_err(|_| "HLS 播放列表格式无效")? {
        m3u8_rs::Playlist::MediaPlaylist(list) => {
            if !list.end_list {
                return Err("直播或未结束的 HLS 不支持计算固定时长".into());
            }
            if list
                .segments
                .iter()
                .any(|s| !s.duration.is_finite() || s.duration <= 0.0)
            {
                return Err("HLS 分片时长无效".into());
            }
            Ok(PlaylistStep::Duration(
                list.segments.iter().map(|s| s.duration as f64).sum(),
            ))
        }
        m3u8_rs::Playlist::MasterPlaylist(list) => {
            let variant = list
                .variants
                .iter()
                .filter(|v| !v.is_i_frame)
                .min_by_key(|v| v.bandwidth)
                .ok_or("HLS 没有可用的视频流")?;
            Ok(PlaylistStep::Variant(variant.uri.clone()))
        }
    }
}
fn hls(
    client: &Client,
    mut url: Url,
    mut bytes: Vec<u8>,
    cancel: &CancellationToken,
) -> Result<f64, String> {
    let mut visited = HashSet::new();
    for _ in 0..5 {
        if cancel.is_cancelled() {
            return Err("已取消".into());
        }
        if !public_url(&url) || !visited.insert(url.clone()) {
            return Err("HLS 播放列表地址无效".into());
        }
        if bytes.is_empty() {
            let response = client
                .get(url.clone())
                .send()
                .map_err(|_| "HLS 播放列表读取失败")?;
            if !response.status().is_success() {
                return Err("HLS 播放列表读取失败".into());
            }
            bytes = read_body(response, 2 * 1024 * 1024)?;
        }
        match parse_hls(&bytes)? {
            PlaylistStep::Duration(duration) => return Ok(duration),
            PlaylistStep::Variant(uri) => {
                url = url.join(&uri).map_err(|_| "HLS 视频流地址无效")?;
                bytes.clear();
            }
        }
    }
    Err("HLS 播放列表嵌套过深".into())
}

#[cfg(test)]
mod tests {
    use super::super::tests::Server;
    use super::*;
    use mp4::WriteBox;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn movie(front: bool, padding: usize) -> Vec<u8> {
        let mut data = vec![];
        mp4::FtypBox::default().write_box(&mut data).unwrap();
        let mut moov = mp4::MoovBox::default();
        moov.mvhd.duration = 125500;
        if front {
            moov.write_box(&mut data).unwrap();
        }
        data.extend_from_slice(&((padding + 8) as u32).to_be_bytes());
        data.extend_from_slice(b"mdat");
        data.resize(data.len() + padding, 0);
        if !front {
            moov.write_box(&mut data).unwrap();
        }
        data
    }

    #[test]
    fn mp4_front_and_tail_metadata_use_bounded_ranges() {
        for front in [true, false] {
            let movie = movie(front, 4 * 1024 * 1024);
            let transferred = Arc::new(AtomicUsize::new(0));
            let counter = transferred.clone();
            let server = Server::bytes(move |request| {
                assert!(!request.headers.contains_key("cookie"));
                assert_eq!(
                    request.headers["user-agent"],
                    super::super::user_agent::DESKTOP
                );
                let (start, end) = request.headers["range"]
                    .strip_prefix("bytes=")
                    .unwrap()
                    .split_once('-')
                    .unwrap();
                let start: usize = start.parse().unwrap();
                let end = end.parse::<usize>().unwrap().min(movie.len() - 1);
                counter.fetch_add(end - start + 1, Ordering::SeqCst);
                (
                    206,
                    movie[start..=end].to_vec(),
                    format!("Content-Range: bytes {start}-{end}/{}\r\n", movie.len()),
                )
            });
            let reader = RemoteReader::new(
                media_client().unwrap(),
                format!("{}/video", server.base).parse().unwrap(),
                CancellationToken::new(),
            )
            .unwrap();
            let length = reader.len;
            assert_eq!(
                mp4::Mp4Reader::read_header(reader, length)
                    .unwrap()
                    .duration()
                    .as_secs_f64(),
                125.5
            );
            assert!(transferred.load(Ordering::SeqCst) < 200_000);
        }
    }

    #[test]
    fn range_fallback_is_bounded_and_cancellation_stops_reads() {
        let data = movie(false, 8);
        let server = Server::bytes(move |_| (200, data.clone(), String::new()));
        let token = CancellationToken::new();
        let mut reader = RemoteReader::new(
            media_client().unwrap(),
            server.base.parse().unwrap(),
            token.clone(),
        )
        .unwrap();
        assert!(reader.complete);
        token.cancel();
        assert!(reader.read(&mut [0; 8]).is_err());
        let server = Server::bytes(|_| (200, vec![0; LIMIT + 1], String::new()));
        assert!(RemoteReader::new(
            media_client().unwrap(),
            server.base.parse().unwrap(),
            CancellationToken::new()
        )
        .is_err());
        let server =
            Server::bytes(|_| (206, vec![0; 20], "Content-Range: bytes 2-21/100\r\n".into()));
        assert!(RemoteReader::new(
            media_client().unwrap(),
            server.base.parse().unwrap(),
            CancellationToken::new()
        )
        .is_err());
    }

    #[test]
    fn hls_master_and_vod_are_parsed_without_fetching_segments() {
        let master = b"#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=200000\nhigh/index.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=100000\nlow/index.m3u8\n";
        assert_eq!(
            parse_hls(master).unwrap(),
            PlaylistStep::Variant("low/index.m3u8".into())
        );
        let vod = b"#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXTINF:9.5,\n1.ts\n#EXTINF:8.25,\n2.ts\n#EXT-X-ENDLIST\n";
        assert_eq!(
            hls(
                &media_client().unwrap(),
                "https://example.org/video/index.m3u8".parse().unwrap(),
                vod.to_vec(),
                &CancellationToken::new()
            )
            .unwrap(),
            17.75
        );
        assert!(parse_hls(b"#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXTINF:9,\n1.ts\n").is_err());
        for address in [
            "file:///tmp/media",
            "http://127.0.0.1/video",
            "https://localhost/video",
            "https://user:pass@example.org/video",
        ] {
            assert!(!public_url(&address.parse().unwrap()));
        }
    }
}

fn media_client() -> Result<Client, String> {
    crate::netproxy::blocking_client_builder()
        .user_agent(super::user_agent::DESKTOP)
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !public_url(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| "无法初始化视频元数据读取器".into())
}

pub(super) async fn probe(source: String, cancel: CancellationToken) -> Result<f64, String> {
    // Blocking HTTP and the MP4 reader are both created/dropped on the blocking pool.
    tokio::task::spawn_blocking(move || {
        let url = Url::parse(&source).map_err(|_| "视频地址无效")?;
        if !public_url(&url) {
            return Err("不支持的视频地址".into());
        }
        let client = media_client()?;
        let reader = RemoteReader::new(client.clone(), url.clone(), cancel.clone())?;
        if reader.bytes.starts_with(b"#EXTM3U") {
            let bytes = if reader.complete || reader.bytes.len() as u64 == reader.len {
                reader.bytes
            } else {
                vec![]
            };
            return hls(&client, url, bytes, &cancel);
        }
        let len = reader.len;
        let mp4 = mp4::Mp4Reader::read_header(reader, len)
            .map_err(|_| "无法解析 MP4 时长（格式、Range 或元数据读取失败）")?;
        if cancel.is_cancelled() {
            return Err("已取消".into());
        }
        Ok(mp4.duration().as_secs_f64())
    })
    .await
    .map_err(|_| "视频时长解析任务失败")?
}
