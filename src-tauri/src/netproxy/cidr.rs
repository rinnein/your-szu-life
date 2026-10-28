//! 网段匹配。
//!
//! 供两处复用：项目根目录 `campus-network.json` 里的校内网白名单，
//! 以及 `no_proxy` 环境变量里的 CIDR 写法。
//!
//! 白名单有近两千条，逐请求解析文本会很浪费，因此提供 [`compile`] 把
//! 网段预先解析成掩码整数，之后每次判断只做整数比较。

use std::net::IpAddr;

/// 已解析的网段集合，按地址族分开存 `(网络地址, 掩码)`。
#[derive(Debug, Default)]
pub(super) struct Compiled {
    ipv4: Vec<(u32, u32)>,
    ipv6: Vec<(u128, u128)>,
}

impl Compiled {
    pub(super) fn matches(&self, host: IpAddr) -> bool {
        match host {
            IpAddr::V4(host) => self
                .ipv4
                .iter()
                .any(|&(base, mask)| (u32::from(host) & mask) == base),
            IpAddr::V6(host) => self
                .ipv6
                .iter()
                .any(|&(base, mask)| (u128::from(host) & mask) == base),
        }
    }
}

/// 把网段文本列表解析成可复用的匹配器；解析不了的条目直接忽略。
pub(super) fn compile(networks: &[String]) -> Compiled {
    let mut compiled = Compiled::default();
    for network in networks {
        match network.trim().parse::<IpAddr>() {
            Ok(IpAddr::V4(base)) => compiled.ipv4.push((u32::from(base), u32::MAX)),
            Ok(IpAddr::V6(base)) => compiled.ipv6.push((u128::from(base), u128::MAX)),
            Err(_) => {
                let Some((base, bits)) = network.split_once('/') else {
                    continue;
                };
                let (Ok(base), Ok(bits)) = (base.trim().parse::<IpAddr>(), bits.trim().parse::<u32>())
                else {
                    continue;
                };
                match base {
                    IpAddr::V4(base) if bits <= 32 => compiled.ipv4.push(mask_v4(base, bits)),
                    IpAddr::V6(base) if bits <= 128 => compiled.ipv6.push(mask_v6(base, bits)),
                    _ => {}
                }
            }
        }
    }
    compiled
}

/// 单条网段判断。`no_proxy` 这类短列表直接用它即可。
pub(super) fn contains(host: IpAddr, network: &str) -> bool {
    let network = network.trim();
    if network.is_empty() {
        return false;
    }
    let (base, bits) = match network.split_once('/') {
        Some((base, bits)) => (base.trim(), bits.trim().parse::<u32>().ok()),
        // 只有地址没有掩码，按单地址精确匹配。
        None => (network, None),
    };
    let Ok(base) = base.parse::<IpAddr>() else {
        return false;
    };
    let Some(bits) = bits else {
        return base == host;
    };
    match (base, host) {
        (IpAddr::V4(base), IpAddr::V4(host)) => {
            if bits > 32 {
                return false;
            }
            let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
            (u32::from(host) & mask) == (u32::from(base) & mask)
        }
        (IpAddr::V6(base), IpAddr::V6(host)) => {
            if bits > 128 {
                return false;
            }
            let mask = if bits == 0 { 0 } else { u128::MAX << (128 - bits) };
            (u128::from(host) & mask) == (u128::from(base) & mask)
        }
        _ => false,
    }
}

fn mask_v4(base: std::net::Ipv4Addr, bits: u32) -> (u32, u32) {
    let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
    (u32::from(base) & mask, mask)
}

fn mask_v6(base: std::net::Ipv6Addr, bits: u32) -> (u128, u128) {
    let mask = if bits == 0 { 0 } else { u128::MAX << (128 - bits) };
    (u128::from(base) & mask, mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap()
    }

    #[test]
    fn matches_ipv4_networks() {
        assert!(contains(ip("172.30.255.42"), "172.16.0.0/12"));
        assert!(contains(ip("172.16.0.1"), "172.16.0.0/12"));
        assert!(contains(ip("172.31.255.255"), "172.16.0.0/12"));
        assert!(!contains(ip("172.32.0.1"), "172.16.0.0/12"));
        assert!(!contains(ip("10.0.0.1"), "172.16.0.0/12"));
        assert!(contains(ip("192.168.84.3"), "192.168.84.0/24"));
        assert!(contains(ip("10.1.2.3"), "10.0.0.0/8"));
    }

    #[test]
    fn matches_bare_addresses_and_rejects_garbage() {
        assert!(contains(ip("10.1.2.3"), "10.1.2.3"));
        assert!(!contains(ip("10.1.2.4"), "10.1.2.3"));
        assert!(!contains(ip("10.1.2.3"), ""));
        assert!(!contains(ip("10.1.2.3"), "10.0.0.0/33"));
        assert!(!contains(ip("10.1.2.3"), "example.com"));
        assert!(!contains(ip("fd00::1"), "10.0.0.0/8"));
    }

    #[test]
    fn matches_ipv6_networks() {
        assert!(contains(ip("fd00::1"), "fd00::/8"));
        assert!(!contains(ip("fe80::1"), "fd00::/8"));
        assert!(contains(ip("2001:db8::1"), "::/0"));
        // 学校数据里用的是大写十六进制。
        assert!(contains(ip("2001:250:3c00::1"), "2001:250:3C00::/48"));
        assert!(!contains(ip("2001:250:3d00::1"), "2001:250:3C00::/48"));
    }

    #[test]
    fn compiled_matches_the_single_network_check() {
        let networks = ["172.21.0.0/16", "192.168.100.0/24", "2001:250:3c00::/48"]
            .map(str::to_owned)
            .to_vec();
        let compiled = compile(&networks);
        for probe in [
            "172.21.5.5",
            "192.168.100.128",
            "2001:250:3c00::1",
        ] {
            assert!(compiled.matches(ip(probe)), "{probe}");
        }
        for probe in ["172.20.0.1", "192.168.101.1", "2001:250:3d00::1", "10.0.0.1"] {
            assert!(!compiled.matches(ip(probe)), "{probe}");
        }
    }

    #[test]
    fn compile_ignores_unparseable_entries() {
        let networks = ["", "not-a-network", "10.0.0.0/33", "172.21.0.0/16"]
            .map(str::to_owned)
            .to_vec();
        let compiled = compile(&networks);
        assert!(compiled.matches(ip("172.21.0.1")));
        assert!(!compiled.matches(ip("10.0.0.1")));
    }
}