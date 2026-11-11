//! Windows 系统代理探测：读注册表里 Internet Settings 的代理项。
//!
//! 与 macOS 同理，统一拦截器关掉了 reqwest 自带的系统代理探测，这里自己读。
//! 用 `RegGetValueW` 在进程内读取，而不是起 `reg query` 子进程：发布版是 GUI
//! 子系统，控制台子进程会在桌面上闪一个窗口，而这条路径每 30 秒就会被代理决策走一次。

use crate::netproxy::system::parse_proxy_value;
use reqwest::Url;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Registry::{
    RegGetValueW, HKEY_CURRENT_USER, RRF_NOEXPAND, RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ,
    RRF_RT_REG_SZ,
};

/// `Internet Settings` 子键，相对 `HKEY_CURRENT_USER`。
#[cfg(target_os = "windows")]
const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

#[cfg(target_os = "windows")]
pub fn system_proxy(scheme: &str) -> Option<Url> {
    // 只认静态代理：PAC（AutoConfigURL）需要执行脚本，这里不做。
    let enable = reg_dword(INTERNET_SETTINGS, "ProxyEnable")?;
    let server = reg_string(INTERNET_SETTINGS, "ProxyServer");
    proxy_from_registry(enable, server.as_deref(), scheme)
}

/// 「ProxyEnable 打开且给出了 ProxyServer」才是一个可用的代理。
///
/// 判断与注册表读取分开，好让非 Windows 的测试也能覆盖这条规则。
fn proxy_from_registry(enable: u32, server: Option<&str>, scheme: &str) -> Option<Url> {
    if enable == 0 {
        return None;
    }
    parse_windows_proxy(server?, scheme)
}

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 读一个 `REG_DWORD`；值不存在或类型不符时返回 `None`。
#[cfg(target_os = "windows")]
fn reg_dword(subkey: &str, name: &str) -> Option<u32> {
    let subkey = wide(subkey);
    let name = wide(name);
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut value as *mut u32 as *mut std::ffi::c_void,
            &mut size,
        )
    };
    (status == ERROR_SUCCESS).then_some(value)
}

/// 读一个 `REG_SZ` / `REG_EXPAND_SZ` 字符串；值不存在或类型不符时返回 `None`。
///
/// `RRF_NOEXPAND` 让 `REG_EXPAND_SZ` 保持原始文本，与过去 `reg query` 的输出一致。
#[cfg(target_os = "windows")]
fn reg_string(subkey: &str, name: &str) -> Option<String> {
    let subkey = wide(subkey);
    let name = wide(name);
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
    // 先问字节长度，再按长度取内容；两次调用之间值变大时第二次会失败，按读不到处理。
    let mut size = 0u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            flags,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS || size < 2 {
        return None;
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            flags,
            std::ptr::null_mut(),
            buffer.as_mut_ptr() as *mut std::ffi::c_void,
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16(&buffer[..len]).ok()
}

/// `ProxyServer` 可能是单地址，也可能是 `http=host:port;https=host:port;ftp=...`。
fn parse_windows_proxy(value: &str, scheme: &str) -> Option<Url> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if !value.contains('=') {
        return parse_proxy_value(value, "http");
    }
    let preferred = if scheme == "https" { "https" } else { "http" };
    let mut fallback = None;
    for entry in value.split(';') {
        let Some((name, address)) = entry.split_once('=') else {
            continue;
        };
        let parsed = parse_proxy_value(address.trim(), "http");
        if name.trim().eq_ignore_ascii_case(preferred) {
            return parsed;
        }
        if fallback.is_none() {
            fallback = parsed;
        }
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_requires_the_enable_flag_and_a_server() {
        assert!(proxy_from_registry(0, Some("127.0.0.1:7890"), "http").is_none());
        // 开关打开但没有 ProxyServer：不可用，不能退化成「直连以外」的猜测。
        assert!(proxy_from_registry(1, None, "http").is_none());
        assert_eq!(
            proxy_from_registry(1, Some("127.0.0.1:7890"), "https").map(|url| url.to_string()),
            Some("http://127.0.0.1:7890/".into())
        );
    }

    #[test]
    fn parses_a_single_proxy_server() {
        assert_eq!(
            parse_windows_proxy("127.0.0.1:7890", "https").map(|url| url.to_string()),
            Some("http://127.0.0.1:7890/".into())
        );
    }

    #[test]
    fn parses_a_per_scheme_proxy_list() {
        let value = "http=127.0.0.1:7890;https=127.0.0.1:7891";
        assert_eq!(
            parse_windows_proxy(value, "https").map(|url| url.to_string()),
            Some("http://127.0.0.1:7891/".into())
        );
        assert_eq!(
            parse_windows_proxy(value, "http").map(|url| url.to_string()),
            Some("http://127.0.0.1:7890/".into())
        );
        // 只配了 ftp 时退回第一个可用项，而不是直接失败。
        assert_eq!(
            parse_windows_proxy("ftp=127.0.0.1:7890", "http").map(|url| url.to_string()),
            Some("http://127.0.0.1:7890/".into())
        );
    }

    #[test]
    fn empty_values_are_rejected() {
        assert!(parse_windows_proxy("", "http").is_none());
        assert!(parse_windows_proxy("   ", "http").is_none());
    }
}
