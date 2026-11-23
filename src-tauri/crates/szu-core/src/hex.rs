//! 小写十六进制编码。
//!
//! `digest` 0.11（sha1/sha2 0.11 与 hmac 0.13 使用的版本）的摘要输出
//! `hybrid_array::Array` 不再实现 `LowerHex`，`format!("{:x}", hasher.finalize())`
//! 会编译失败；需要摘要字符串的地方统一走这里。

/// 把字节序列编码成小写十六进制字符串。
pub fn encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn encodes_lowercase_hex() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(&[0x00, 0x0f, 0x10, 0xff]), "000f10ff");
    }
}
