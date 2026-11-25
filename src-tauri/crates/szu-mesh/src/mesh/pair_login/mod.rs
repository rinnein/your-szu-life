//! 远程授权登录：在未登录的新设备与已登录设备之间，用一次性的 EasyTier
//! 配对网络传递初始化向导 `auth` 步骤所需的校园卡号与统一认证密码。
//!
//! 传输复用 [`super::share_room`] 的同一套原语（第二个 EasyTier 实例、
//! overlay→回环桥、每 peer 出站桥），但协议是请求/响应 + 一次性授权，不是广播
//! 房间，因此单独成模块，避免把签到逻辑耦合进来。
//!
//! 身份派生：
//! - 二维码 URL 只携带 32 字节随机 secret；
//! - 网络名 `your-szu-life:login/pair:<hash>` 与界面 6 位码都由 secret 散列得到，
//!   因此网络名与 peer 列表不会泄露密钥；
//! - EasyTier 网络密钥、应用层 token、凭据载荷 AEAD key 都从 secret 再派生。
//!
//! 安全边界：拿到二维码（截图/旁观）的人即可加入配对网络。授权页必须展示对端
//! 设备信息与 6 位码供用户核对；凭据只在一次性 POST 中传输，且先用
//! XChaCha20-Poly1305 加密——即使 EasyTier secure-mode 回退，overlay 上也不会
//! 出现明文密码。
//!
//! 刻意不对 MCP 开放：本模块会读写统一认证明文密码，任何本地 MCP 客户端若能
//! 触发或读取都等于凭据外泄口；流程也必须依赖另一台设备的物理扫码与人工确认，
//! AI 调用没有合法场景。

use std::sync::Mutex;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod client;
pub mod commands;
pub mod host;

/// 配对服务在虚拟网内的端口（overlay 命名空间，与 SYNC_API_PORT /
/// CAMPUS_PROXY_PORT / SHARE_ROOM_PORT 同一惯例；回环端口由 ports 模块分配）。
pub const PAIR_PORT: u16 = 54304;

/// host 端状态变化事件；payload 是 [`PairHostView`]。
pub const PAIR_HOST_EVENT: &str = "pair-login-host-changed";

/// 两个方向都只跑一个临时实例；角色分开命名，避免同机同时承担两种角色时混淆。
pub const HOST_INSTANCE: &str = "your-szu-life-login-pair-host";
pub const CLIENT_INSTANCE: &str = "your-szu-life-login-pair-client";

pub const TOKEN_HEADER: &str = "x-pair-token";

/// 配对链接的固定路径（`your-szu-life://page/pair-login?secret=...`）。
pub const PAIR_PAGE: &str = "/pair-login";

/// 配对房间的有效期。
pub const PAIR_TTL: Duration = Duration::from_secs(10 * 60);

/// 授权端等待主机出现在路由表里的最长时间。
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

const SECRET_HEX_LEN: usize = 64;
const CODE_HASH_LEN: usize = 12;
const DISPLAY_CODE_LEN: usize = 6;
const NONCE_LEN: usize = 24;
const PAYLOAD_MAGIC: &[u8] = b"SZULPAIR1";
const MAX_PASSWORD_CHARS: usize = 256;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn hash_bytes(input: &str) -> [u8; 32] {
    let digest = Sha256::digest(input.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn hash_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// 一次配对会话的全部派生值。
///
/// 不实现 `Debug`：`secret` 是二维码里唯一的能力凭证，不能随日志泄漏。
#[derive(Clone)]
pub struct PairSecret {
    secret: String,
    code: String,
    display_code: String,
    network_name: String,
    network_secret: String,
    token: String,
    payload_key: [u8; 32],
}

impl PairSecret {
    /// 生成新的配对密钥（32 字节随机，hex 64 字符）。
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        Self::from_hex(&to_hex(&bytes)).expect("随机 secret 一定是 64 位 hex")
    }

    /// 从二维码里的 secret 重建会话；大小写归一化。
    pub fn from_hex(raw: &str) -> Result<Self, String> {
        let secret = raw.trim().to_ascii_lowercase();
        if secret.len() != SECRET_HEX_LEN || !secret.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("配对密钥格式无效".into());
        }
        let code_hash = hash_hex(&format!("your-szu-life:login-pair:code:v1:{secret}"));
        let code = code_hash[..CODE_HASH_LEN].to_owned();
        let display_code = code[..DISPLAY_CODE_LEN].to_ascii_uppercase();
        let network_name = format!("your-szu-life:login/pair:{code}");
        let network_secret = hash_hex(&format!("your-szu-life:login-pair:net:v1:{secret}"));
        let token = super::api::derive_token(&network_secret);
        let payload_key = hash_bytes(&format!("your-szu-life:login-pair:payload:v1:{secret}"));
        Ok(Self {
            secret,
            code,
            display_code,
            network_name,
            network_secret,
            token,
            payload_key,
        })
    }

    /// 解析深链或设置页粘贴的配对链接。
    ///
    /// 接受 `your-szu-life://page/pair-login?secret=...` 与 `/pair-login?secret=...`；
    /// 路径末尾必须是 [`PAIR_PAGE`]，其余查询参数忽略。
    pub fn parse_invite(input: &str) -> Result<Self, String> {
        let raw = input.trim();
        if raw.is_empty() {
            return Err("配对链接为空".into());
        }
        let (target, query) = raw
            .split_once('?')
            .ok_or_else(|| "配对链接缺少密钥".to_string())?;
        let target = target.trim_end_matches('/');
        if !target.ends_with(PAIR_PAGE) {
            return Err("配对链接无效".into());
        }
        let query = query.split('#').next().unwrap_or(query);
        let secret = query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| *key == "secret")
            .map(|(_, value)| value)
            .ok_or_else(|| "配对链接缺少密钥".to_string())?;
        Self::from_hex(secret)
    }

    pub fn deep_link(&self) -> String {
        format!("your-szu-life://page/pair-login?secret={}", self.secret)
    }

    /// EasyTier 网络身份；学号只用于展示，配对网络没有账户概念。
    pub fn identity(&self) -> super::identity::MeshIdentity {
        super::identity::MeshIdentity {
            network_name: self.network_name.clone(),
            network_secret: self.network_secret.clone(),
            student_id: String::new(),
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn display_code(&self) -> &str {
        &self.display_code
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn network_name(&self) -> &str {
        &self.network_name
    }

    pub fn network_secret(&self) -> &str {
        &self.network_secret
    }

    /// 加密凭据载荷（AAD 固定为协议标记，防止跨协议复用）。
    pub fn seal(&self, credentials: &PairCredentials) -> Result<PairSealedPayload, String> {
        let plain =
            serde_json::to_vec(credentials).map_err(|error| format!("凭据序列化失败: {error}"))?;
        let mut nonce = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce);
        let ciphertext = XChaCha20Poly1305::new((&self.payload_key).into())
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &plain,
                    aad: PAYLOAD_MAGIC,
                },
            )
            .map_err(|_| "凭据加密失败".to_string())?;
        Ok(PairSealedPayload {
            nonce: BASE64.encode(nonce),
            ciphertext: BASE64.encode(ciphertext),
        })
    }

    /// 解密并校验凭据载荷。
    pub fn open(&self, sealed: &PairSealedPayload) -> Result<PairCredentials, String> {
        let raw_nonce = BASE64
            .decode(sealed.nonce.as_bytes())
            .map_err(|_| "配对载荷格式无效".to_string())?;
        let nonce: [u8; NONCE_LEN] = raw_nonce
            .as_slice()
            .try_into()
            .map_err(|_| "配对载荷格式无效".to_string())?;
        let ciphertext = BASE64
            .decode(sealed.ciphertext.as_bytes())
            .map_err(|_| "配对载荷格式无效".to_string())?;
        let plain = XChaCha20Poly1305::new((&self.payload_key).into())
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &ciphertext,
                    aad: PAYLOAD_MAGIC,
                },
            )
            .map_err(|_| "配对载荷解密失败".to_string())?;
        let credentials: PairCredentials =
            serde_json::from_slice(&plain).map_err(|_| "配对载荷内容无效".to_string())?;
        credentials.validate()?;
        Ok(credentials.normalized())
    }
}

/// 授权端发给 host 的凭据（仅在 AEAD 载荷与内存中出现）。
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairCredentials {
    pub campus_card_number: String,
    pub password: String,
}

impl PairCredentials {
    pub fn validate(&self) -> Result<(), String> {
        if !szu_core::auth::model::is_campus_card_number(self.campus_card_number.trim()) {
            return Err("校园卡号必须是 6 位数字".into());
        }
        if self.password.is_empty() {
            return Err("密码不能为空".into());
        }
        if self.password.chars().count() > MAX_PASSWORD_CHARS {
            return Err("密码长度异常".into());
        }
        Ok(())
    }

    fn normalized(mut self) -> Self {
        self.campus_card_number = self.campus_card_number.trim().to_owned();
        self
    }
}

/// 暴露给前端的一次性凭据视图。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairCredentialsView {
    pub campus_card_number: String,
    pub password: String,
}

impl From<PairCredentials> for PairCredentialsView {
    fn from(value: PairCredentials) -> Self {
        Self {
            campus_card_number: value.campus_card_number,
            password: value.password,
        }
    }
}

/// 线上传输的加密载荷（base64）。
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairSealedPayload {
    pub nonce: String,
    pub ciphertext: String,
}

/// 对端设备的最小自我描述（用于授权页展示）。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PairDevice {
    pub name: String,
    pub platform: String,
    pub app_version: String,
}

/// host 的 `/v1/pair/info` 响应。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairInfoResponse {
    pub protocol: u32,
    pub code: String,
    pub expires_at_ms: i64,
    pub device: PairDevice,
}

/// 协议版本，host 与 client 必须一致。
pub const PAIR_PROTOCOL: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PairHostStatus {
    Waiting,
    Connected,
    Accepted,
    Expired,
}

/// 向导里 host 端的状态视图，也是事件 payload。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairHostView {
    pub status: PairHostStatus,
    pub code: String,
    pub deep_link: String,
    pub expires_at_ms: i64,
}

/// 授权端连接成功后的视图。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairPeerView {
    pub code: String,
    pub device: PairDevice,
}

/// 配对会话的唯一持有者，作为 Tauri state 管理。
pub struct PairLoginStore {
    pub(crate) host: tokio::sync::Mutex<Option<host::HostSession>>,
    pub(crate) client: tokio::sync::Mutex<Option<client::ClientSession>>,
    pub(crate) invite: Mutex<Option<client::StoredInvite>>,
}

impl PairLoginStore {
    pub fn new() -> Self {
        Self {
            host: tokio::sync::Mutex::new(None),
            client: tokio::sync::Mutex::new(None),
            invite: Mutex::new(None),
        }
    }
}

impl Default for PairLoginStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> PairCredentials {
        PairCredentials {
            campus_card_number: "123456".into(),
            password: "password-1".into(),
        }
    }

    #[test]
    fn secret_derivation_is_stable_and_session_specific() {
        let raw = "a".repeat(64);
        let a = PairSecret::from_hex(&raw).expect("合法 secret");
        let b = PairSecret::from_hex(&raw.to_uppercase()).expect("大小写归一化");
        assert_eq!(a.code(), b.code());
        assert_eq!(a.network_name(), b.network_name());
        assert_eq!(a.network_secret(), b.network_secret());
        assert_eq!(a.token(), b.token());
        assert_eq!(a.code().len(), CODE_HASH_LEN);
        assert_eq!(a.display_code().len(), DISPLAY_CODE_LEN);
        assert_eq!(a.display_code(), a.code()[..DISPLAY_CODE_LEN].to_uppercase());
        assert_eq!(a.network_name(), format!("your-szu-life:login/pair:{}", a.code()));
        assert_eq!(a.network_secret().len(), 64);
        assert_eq!(a.token().len(), 64);
        assert!(!a.deep_link().contains(a.network_secret()));

        let other = PairSecret::from_hex(&"b".repeat(64)).expect("另一个 secret");
        assert_ne!(a.code(), other.code());
        assert_ne!(a.network_secret(), other.network_secret());
        assert_ne!(a.token(), other.token());
    }

    #[test]
    fn generated_secrets_are_valid_and_distinct() {
        let a = PairSecret::generate();
        let b = PairSecret::generate();
        assert_ne!(a.code(), b.code());
        assert_eq!(a.code().len(), CODE_HASH_LEN);
        assert!(a.deep_link().starts_with("your-szu-life://page/pair-login?secret="));
    }

    #[test]
    fn invite_urls_accept_scheme_route_and_extra_params() {
        let raw = "c".repeat(64);
        let expected = PairSecret::from_hex(&raw).unwrap();
        for input in [
            format!("your-szu-life://page/pair-login?secret={raw}"),
            format!("your-szu-life://page/pair-login/?secret={raw}&v=1"),
            format!("/pair-login?secret={raw}"),
            format!("  /pair-login?secret={raw}#ignored  "),
        ] {
            let parsed = PairSecret::parse_invite(&input).expect(&input);
            assert_eq!(parsed.code(), expected.code(), "{input}");
        }
    }

    #[test]
    fn invite_urls_reject_bad_shapes() {
        for input in [
            "",
            "your-szu-life://page/pair-login",
            "your-szu-life://page/other?secret=aa",
            "/pair-login?secret=zz",
            "/pair-login?secret=abc",
            "https://example.com/pair-login?secret=aa",
        ] {
            assert!(PairSecret::parse_invite(input).is_err(), "{input}");
        }
    }

    #[test]
    fn payload_round_trip_and_wrong_secret_is_rejected() {
        let secret = PairSecret::from_hex(&"d".repeat(64)).unwrap();
        let sealed = secret.seal(&credentials()).expect("加密成功");
        assert!(!sealed.ciphertext.contains("password-1"));
        let opened = secret.open(&sealed).expect("解密成功");
        assert_eq!(opened.campus_card_number, "123456");
        assert_eq!(opened.password, "password-1");

        let other = PairSecret::from_hex(&"e".repeat(64)).unwrap();
        assert!(other.open(&sealed).is_err());

        let tampered = PairSealedPayload {
            nonce: sealed.nonce,
            ciphertext: BASE64.encode([0u8; 32]),
        };
        assert!(secret.open(&tampered).is_err());
    }

    #[test]
    fn credentials_are_validated() {
        let mut value = credentials();
        value.campus_card_number = "12345".into();
        assert!(value.validate().is_err());
        let mut value = credentials();
        value.campus_card_number = "12345a".into();
        assert!(value.validate().is_err());
        let mut value = credentials();
        value.password.clear();
        assert!(value.validate().is_err());
        let mut value = credentials();
        value.password = "x".repeat(MAX_PASSWORD_CHARS + 1);
        assert!(value.validate().is_err());
        assert!(credentials().validate().is_ok());
    }

    #[test]
    fn trimmed_campus_card_is_normalized_on_open() {
        let secret = PairSecret::from_hex(&"f".repeat(64)).unwrap();
        let mut value = credentials();
        value.campus_card_number = " 123456 ".into();
        let sealed = secret.seal(&value).unwrap();
        let opened = secret.open(&sealed).unwrap();
        assert_eq!(opened.campus_card_number, "123456");
    }

    #[test]
    fn network_name_does_not_contain_the_secret() {
        let secret = PairSecret::from_hex(&"9".repeat(64)).unwrap();
        assert!(!secret.network_name().contains(&"9".repeat(32)));
        assert!(!secret.deep_link().contains(secret.network_secret()));
    }
}
