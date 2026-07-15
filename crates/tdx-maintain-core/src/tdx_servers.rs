//! Parse TDX client `connect.cfg` to extract HQ server addresses.
//!
//! When `rustdx::tcp::Tcp::new()` fails (its 19 hardcoded IPs are outdated),
//! this module reads the user's TDX client config to find working servers.

use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use tracing::info;

/// A server address parsed from connect.cfg
#[derive(Debug, Clone)]
pub struct TdxServer {
    pub name: String,
    pub addr: SocketAddr,
}

/// Parse `connect.cfg` from `tdx_data_dir` and return the HQ host list.
pub fn parse_connect_cfg(tdx_data_dir: &Path) -> Vec<TdxServer> {
    let cfg_path = tdx_data_dir.join("connect.cfg");
    if !cfg_path.exists() {
        info!("connect.cfg not found at {:?}, using default servers", cfg_path);
        return vec![];
    }

    let content = match std::fs::read_to_string(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            info!("Failed to read connect.cfg: {}", e);
            return vec![];
        }
    };

    parse_hq_host_section(&content)
}

fn parse_hq_host_section(content: &str) -> Vec<TdxServer> {
    // Determine if we're inside [HQHOST] section
    let mut in_hq_host = false;
    let mut ip_map: Vec<(String, String)> = Vec::new(); // (num_str, ip)
    let mut port_map: Vec<(String, u16)> = Vec::new();  // (num_str, port)
    let mut name_map: Vec<(String, String)> = Vec::new(); // (num_str, name)

    for line in content.lines() {
        let trimmed = line.trim();

        // Empty line or comment
        if trimmed.is_empty() {
            continue;
        }

        // Section header
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_hq_host = trimmed.eq_ignore_ascii_case("[HQHOST]");
            continue;
        }

        if !in_hq_host {
            continue;
        }

        // Parse key=value pairs
        if let Some((key, value)) = parse_key_value(trimmed) {
            if let Some(num) = extract_number(&key, "IPAddress") {
                ip_map.push((num, value));
            } else if let Some(num) = extract_number(&key, "Port") {
                if let Ok(port) = value.parse::<u16>() {
                    port_map.push((num, port));
                }
            } else if let Some(num) = extract_number(&key, "HostName") {
                name_map.push((num, value));
            }
        }
    }

    // Merge by number suffix
    let mut servers = Vec::new();
    for (num, ip_str) in &ip_map {
        let port = port_map.iter()
            .find(|(n, _)| n == num)
            .map(|(_, p)| *p)
            .unwrap_or(7709); // default TDX port

        let name = name_map.iter()
            .find(|(n, _)| n == num)
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| format!("TDX-{}", num));

        if let Ok(ip) = ip_str.parse::<IpAddr>() {
            servers.push(TdxServer {
                name,
                addr: SocketAddr::new(ip, port),
            });
        }
    }

    servers
}

/// Parse a line like "IPAddress01=110.41.147.114" into ("IPAddress", "110.41.147.114")
fn parse_key_value(line: &str) -> Option<(String, String)> {
    let mut parts = line.splitn(2, '=');
    let key = parts.next()?.trim().to_string();
    let value = parts.next()?.trim().to_string();
    if key.is_empty() {
        return None;
    }
    Some((key, value))
}

/// If key starts with `prefix` followed by digits, return those digits as a string.
/// e.g. extract_number("IPAddress01", "IPAddress") -> Some("01")
fn extract_number(key: &str, prefix: &str) -> Option<String> {
    if let Some(suffix) = key.strip_prefix(prefix) {
        if suffix.chars().all(|c| c.is_ascii_digit()) && !suffix.is_empty() {
            return Some(suffix.to_string());
        }
    }
    None
}

/// Try each candidate via raw TCP connect (fast pre-filter).
///
/// Uses a per-address connect-timeout of 2s.
/// Returns `None` if no server responds.
/// Prefer `find_working_server_rustdx` for real TDX protocol probing.
pub fn find_working_server(candidates: &[SocketAddr]) -> Option<SocketAddr> {
    use std::net::TcpStream;
    use std::time::Duration;

    const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

    for addr in candidates {
        match TcpStream::connect_timeout(addr, CONNECT_TIMEOUT) {
            Ok(_) => {
                info!("TDX server TCP reachable: {}", addr);
                return Some(*addr);
            }
            Err(e) => {
                info!("TDX server {} unreachable: {}", addr, e);
            }
        }
    }

    None
}

/// Try each candidate server with a real rustdx `Tcp::new_with_ip()` connection.
///
/// This verifies the server speaks TDX protocol, not just TCP.
/// Returns the first working `SocketAddr`, or `None` if no server accepts TDX connections.
pub fn find_working_server_rustdx(candidates: &[SocketAddr]) -> Option<SocketAddr> {
    if candidates.is_empty() {
        return None;
    }

    // Quick TCP pre-filter: only try rustdx on servers that pass TCP connect
    let reachable: Vec<SocketAddr> = candidates
        .iter()
        .filter(|addr| {
            use std::net::TcpStream;
            use std::time::Duration;
            TcpStream::connect_timeout(addr, Duration::from_secs(2)).is_ok()
        })
        .copied()
        .collect();

    if reachable.is_empty() {
        // No TCP-reachable server at all — try the first few candidates anyway
        let few: Vec<_> = candidates.iter().take(5).copied().collect();
        for addr in &few {
            match rustdx::tcp::Tcp::new_with_ip(addr) {
                Ok(_) => {
                    info!("TDX server protocol OK (no TCP pre-check): {}", addr);
                    return Some(*addr);
                }
                Err(e) => {
                    info!("TDX server {} protocol failed: {}", addr, e);
                }
            }
        }
        return None;
    }

    // Try rustdx connection on each TCP-reachable server
    for addr in &reachable {
        match rustdx::tcp::Tcp::new_with_ip(addr) {
            Ok(_) => {
                info!("TDX server protocol OK: {}", addr);
                return Some(*addr);
            }
            Err(e) => {
                info!("TDX server {} TCP ok but rustdx failed: {}", addr, e);
            }
        }
    }

    None
}

/// Well-known public HQ servers that still serve historical K-lines to
/// third-party clients (rustdx / pytdx). Many newer connect.cfg hosts accept
/// TCP + handshake but return empty bar payloads.
const KLINE_CAPABLE_FALLBACKS: &[&str] = &[
    "180.153.18.170:7709", // 上海电信 (legacy, kline OK)
    "180.153.18.171:7709",
    "115.238.90.165:7709", // 杭州
    "60.191.117.167:7709", // 浙江
    "218.75.126.9:7709",
    "124.223.163.242:7709", // 上海双线 (connect.cfg)
    "110.41.147.114:7709",  // 深圳双线 (connect.cfg)
    "62.234.50.143:7709",   // 北京双线
    "159.75.29.111:7709",   // 广州双线
];

/// Build a prioritized list of server addresses.
///
/// Order: known kline-capable hosts first (many newer connect.cfg HQ hosts
/// accept TCP/handshake but return empty historical bars), then the rest of
/// connect.cfg. Deduplicated by SocketAddr.
pub fn get_server_candidates(tdx_data_dir: &Path) -> Vec<TdxServer> {
    let mut servers = Vec::new();
    let mut seen: std::collections::HashSet<SocketAddr> = std::collections::HashSet::new();

    for ip_str in KLINE_CAPABLE_FALLBACKS {
        if let Ok(addr) = ip_str.parse::<SocketAddr>() {
            if seen.insert(addr) {
                servers.push(TdxServer {
                    name: format!("fallback-{}", addr),
                    addr,
                });
            }
        }
    }

    for s in parse_connect_cfg(tdx_data_dir) {
        if seen.insert(s.addr) {
            servers.push(s);
        }
    }

    if servers.is_empty() {
        info!("No TDX servers found in connect.cfg or fallback list");
    } else {
        info!("Loaded {} TDX server addresses", servers.len());
    }

    servers
}

/// Get server `SocketAddr` list only (convenience wrapper).
pub fn get_server_addrs(tdx_data_dir: &Path) -> Vec<SocketAddr> {
    get_server_candidates(tdx_data_dir)
        .into_iter()
        .map(|s| s.addr)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hq_host_section() {
        let sample = r#"
[HQHOST]
HostNum=2
PrimaryHost=1

HostName01=通达信深圳双线主站1
IPAddress01=110.41.147.114
Port01=7709

HostName02=通达信上海双线主站1
IPAddress02=124.223.163.242
Port02=7709

[OTHER]
foo=bar
"#;
        let servers = parse_hq_host_section(sample);
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "通达信深圳双线主站1");
        assert_eq!(servers[0].addr.to_string(), "110.41.147.114:7709");
        assert_eq!(servers[1].name, "通达信上海双线主站1");
        assert_eq!(servers[1].addr.to_string(), "124.223.163.242:7709");
    }

    #[test]
    fn test_parse_empty_content() {
        let servers = parse_hq_host_section("");
        assert!(servers.is_empty());
    }

    #[test]
    fn test_parse_no_hq_host_section() {
        let content = "[OTHER]\na=b\n";
        let servers = parse_hq_host_section(content);
        assert!(servers.is_empty());
    }
}
