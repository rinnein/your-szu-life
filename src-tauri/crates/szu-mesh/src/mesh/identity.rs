//! 设备自组网身份派生。
//!
//! 有两种身份：
//!
//! - **账号网络**（[`derive`]）：EasyTier 用 `network_name` + `network_secret` 标识
//!   一个虚拟网络，名字会出现在 peer 列表里，因此由学号单向散列得到；密钥参与
//!   握手与数据面加密，由账户资料（学号、校园卡号、统一认证密码）派生。两台设备
//!   只有在资料完全一致时才会落到同一个网络 —— 这正是「已经用 authserver 登录过」
//!   的等价物。
//! - **公共中继网络**（[`public_identity`]）：与账号无关的固定身份，所有安装都
//!   加入同一个网络，只用于互相发现与中转；它不承载本机应用数据面，因此不知道
//!   任何账号网络的密钥。
//!
//! 这里是全应用**唯一**一处派生组网身份的地方：网络名/密钥的算法、分隔符与长度
//! 都只在这里定义，其它模块只消费 [`MeshIdentity`]。

use szu_core::auth::model::StoredAccountProfile;
use szu_core::storage::{CredentialStore, PlatformCredentialStore};
use argon2::{Algorithm, Argon2, Params, Version};
use sha2::{Digest, Sha256};

/// 网络名固定前缀；后面接学号散列的前 16 个十六进制字符。
const NETWORK_NAME_PREFIX: &str = "your-szu-life:self:";
/// 常驻公共中继网络的固定网络名；所有安装都加入同一个网络。
pub const PUBLIC_NETWORK_NAME: &str = "your-szu-life:public:init";
/// 公共网络密钥的派生种子。密钥本身是公开的（任何人都能加入这个中继网络），
/// 但必须跨版本、跨设备完全一致，因此固定成常量而不是随机生成。
const PUBLIC_SECRET_SEED: &str = "your-szu-life:public-init:v1";
const NAME_HASH_LEN: usize = 8;
const SECRET_LEN: usize = 32;
const SECRET_SALT_LEN: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeshIdentity {
    /// 出现在 peer 列表里的网络名；不含任何明文账号信息。
    pub network_name: String,
    /// 握手与数据面加密用的网络密钥；只在本进程内使用。
    pub network_secret: String,
    /// 当前设备身份对应的学号。用于显示与日志定位，不参与网络标识。
    pub student_id: String,
}

/// 读取本机账户资料并派生身份。资料不完整时返回用户可读的原因。
pub fn derive_current() -> Result<MeshIdentity, String> {
    let profile = PlatformCredentialStore
        .load_profile()
        .map_err(|error| format!("读取账户资料失败: {error}"))?
        .ok_or_else(|| "尚未登录，无法启用设备同步".to_string())?;
    derive(&profile)
}

/// 常驻公共中继身份：网络名固定，密钥由固定种子散列得到。
///
/// 任何安装都能算出同一份密钥并加入该网络，这是设计的一部分——它只负责互相
/// 发现与中转。账号网络的数据对它是端到端加密的（secure-mode），它也拿不到
/// 账号网络的密钥。
pub fn public_identity() -> MeshIdentity {
    MeshIdentity {
        network_name: PUBLIC_NETWORK_NAME.to_owned(),
        network_secret: hash_hex(PUBLIC_SECRET_SEED),
        student_id: String::new(),
    }
}

/// 由账户资料派生身份。纯函数，便于测试固定输入。
pub fn derive(profile: &StoredAccountProfile) -> Result<MeshIdentity, String> {
    let student_id = profile.student_id.trim();
    if student_id.is_empty() {
        return Err("账户资料缺少学号，无法启用设备同步".into());
    }
    if profile.password.is_empty() {
        return Err("账户资料缺少统一认证密码，无法启用设备同步".into());
    }

    let name_hash = hash_hex(&format!("your-szu-life:net-name:v1:{student_id}"));
    let network_name = format!(
        "{NETWORK_NAME_PREFIX}{}",
        &name_hash[..NAME_HASH_LEN * 2]
    );
    let network_secret = derive_secret(
        &profile.password,
        student_id,
        profile.campus_card_number.trim(),
    )?;
    Ok(MeshIdentity {
        network_name,
        network_secret,
        student_id: student_id.to_owned(),
    })
}

/// 32 字节密钥的十六进制表示。盐由学号与校园卡号确定，因此同一账户在任何设备上
/// 都会派生出同一个网络密钥。
fn derive_secret(password: &str, student_id: &str, campus_card_number: &str) -> Result<String, String> {
    let salt_material = format!(
        "your-szu-life:net-secret:v1:{student_id}\u{1f}{campus_card_number}"
    );
    let salt = hash_bytes(&salt_material);
    let params = Params::new(19 * 1024, 2, 1, Some(SECRET_LEN))
        .map_err(|_| "组网密钥派生参数无效")?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; SECRET_LEN];
    argon
        .hash_password_into(password.as_bytes(), &salt[..SECRET_SALT_LEN], &mut key)
        .map_err(|_| "组网密钥派生失败")?;
    Ok(to_hex(&key))
}

fn hash_bytes(input: &str) -> [u8; 32] {
    let digest = Sha256::digest(input.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn hash_hex(input: &str) -> String {
    to_hex(&hash_bytes(input))
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(student_id: &str, card: &str, password: &str) -> StoredAccountProfile {
        StoredAccountProfile {
            student_id: student_id.to_owned(),
            campus_card_number: card.to_owned(),
            name: Some("测试用户".into()),
            password: password.to_owned(),
        }
    }

    #[test]
    fn same_profile_derives_the_same_identity() {
        let a = derive(&profile("20240001", "12345678", "password-1")).unwrap();
        let b = derive(&profile("20240001", "12345678", "password-1")).unwrap();
        assert_eq!(a, b);
        assert!(a.network_name.starts_with(NETWORK_NAME_PREFIX));
        assert_eq!(
            a.network_name.len(),
            NETWORK_NAME_PREFIX.len() + NAME_HASH_LEN * 2
        );
        assert_eq!(a.network_secret.len(), SECRET_LEN * 2);
        assert!(a.network_secret.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!a.network_name.contains("20240001"));
        assert!(!a.network_secret.contains("password-1"));
    }

    #[test]
    fn password_change_rotates_the_network_secret() {
        let a = derive(&profile("20240001", "12345678", "password-1")).unwrap();
        let b = derive(&profile("20240001", "12345678", "password-2")).unwrap();
        assert_eq!(a.network_name, b.network_name);
        assert_ne!(a.network_secret, b.network_secret);
    }

    #[test]
    fn different_students_get_different_networks() {
        let a = derive(&profile("20240001", "12345678", "same-password")).unwrap();
        let b = derive(&profile("student-b", "card-b", "pass-x")).unwrap();
        assert_ne!(a.network_name, b.network_name);
        assert_ne!(a.network_secret, b.network_secret);
    }

    #[test]
    fn campus_card_change_rotates_the_secret_but_not_the_name() {
        let a = derive(&profile("20240001", "12345678", "password-1")).unwrap();
        let b = derive(&profile("20240001", "card-b", "pass-x")).unwrap();
        assert_eq!(a.network_name, b.network_name);
        assert_ne!(a.network_secret, b.network_secret);
    }

    #[test]
    fn incomplete_profile_is_rejected() {
        assert!(derive(&profile("", "12345678", "password-1")).is_err());
        assert!(derive(&profile("20240001", "12345678", "")).is_err());
    }

    #[test]
    fn secret_is_32_bytes_of_hex() {
        let identity = derive(&profile("20240001", "12345678", "password-1")).unwrap();
        assert_eq!(identity.network_secret.len(), 64);
    }

    #[test]
    fn public_identity_is_fixed_and_account_independent() {
        let a = public_identity();
        let b = public_identity();
        assert_eq!(a, b);
        assert_eq!(a.network_name, PUBLIC_NETWORK_NAME);
        assert_eq!(a.network_name, "your-szu-life:public:init");
        assert_eq!(a.network_secret.len(), SECRET_LEN * 2);
        assert!(a.network_secret.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(a.student_id.is_empty());

        // 公共身份不能和任何账号网络撞名，否则账号设备会被并进公共网络。
        let account = derive(&profile("20240001", "12345678", "password-1")).unwrap();
        assert_ne!(a.network_name, account.network_name);
        assert_ne!(a.network_secret, account.network_secret);
    }
}
