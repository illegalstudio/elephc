//! Purpose:
//! Derives the PHP-visible connection facts from an optional trusted-proxy
//! header chain: `REMOTE_ADDR`, `REMOTE_PORT`, `REQUEST_SCHEME`, `HTTPS`, and
//! `SERVER_PORT`.
//!
//! Called from:
//! - `crate::worker` and `crate::isolated_worker` once per request, before the
//!   request metadata is stored or sent to a handler process.
//!
//! Key details:
//! - Without `--trusted-proxy`, the socket peer is authoritative and the result
//!   is byte-identical to the pre-feature behavior.
//! - RFC 7239 `Forwarded` wins over `X-Forwarded-*` when present and
//!   well-formed; malformed or obfuscated identifiers fall back to the peer.
//! - The chain is walked right-to-left, so only trusted hops are skipped.

use std::borrow::Cow;
use std::net::{IpAddr, Ipv6Addr};
use std::sync::Arc;

use crate::listener::PeerInfo;

/// Trusted proxy allowlist parsed from one or more `--trusted-proxy` values.
#[derive(Clone, Debug, Default)]
pub(crate) struct TrustedProxies {
    /// Parsed entries; an empty list disables forwarded-header processing.
    entries: Arc<[TrustedProxy]>,
}

/// One trusted proxy entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrustedProxy {
    /// An IPv4 or IPv6 network.
    Network {
        /// Network address (not necessarily masked).
        network: IpAddr,
        /// Prefix length in bits.
        prefix: u8,
    },
    /// A Unix-domain-socket peer.
    Unix,
}

impl TrustedProxies {
    /// Parses comma-separated IP/CIDR entries from every `--trusted-proxy` flag.
    ///
    /// Returns a user-facing message on malformed input so the caller can exit
    /// with the same usage-error code as the other runtime flags.
    pub(crate) fn parse_values(values: &[String]) -> Result<Self, String> {
        let mut entries = Vec::new();
        for value in values {
            for token in value.split(',') {
                let token = token.trim();
                if token.is_empty() {
                    return Err("--trusted-proxy contains an empty entry".to_string());
                }
                entries.push(TrustedProxy::parse(token)?);
            }
        }
        Ok(Self {
            entries: entries.into(),
        })
    }

    /// Returns whether `peer` is allowed to supply forwarded headers.
    fn is_trusted(&self, peer: &PeerInfo) -> bool {
        match peer.remote_ip() {
            Some(ip) => self.is_trusted_ip(ip),
            None => self
                .entries
                .iter()
                .any(|entry| matches!(entry, TrustedProxy::Unix)),
        }
    }

    /// Returns whether an IP falls inside one trusted network.
    fn is_trusted_ip(&self, ip: IpAddr) -> bool {
        self.entries.iter().any(|entry| entry.matches(ip))
    }
}

impl TrustedProxy {
    /// Parses one `IP`, `IP/PREFIX`, or `unix` entry.
    fn parse(token: &str) -> Result<Self, String> {
        if token.eq_ignore_ascii_case("unix") {
            return Ok(Self::Unix);
        }
        let (address, prefix) = match token.split_once('/') {
            Some((address, prefix)) => {
                let prefix = prefix
                    .parse::<u8>()
                    .map_err(|_| format!("invalid --trusted-proxy prefix in {token:?}"))?;
                (address, Some(prefix))
            }
            None => (token, None),
        };
        let network: IpAddr = address
            .parse()
            .map_err(|_| format!("invalid --trusted-proxy address {token:?}"))?;
        let max = match network {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        let prefix = prefix.unwrap_or(max);
        if prefix > max {
            return Err(format!(
                "--trusted-proxy prefix {prefix} exceeds the {max}-bit address in {token:?}"
            ));
        }
        Ok(Self::Network { network, prefix })
    }

    /// Returns whether `ip` falls inside this trusted network.
    fn matches(self, ip: IpAddr) -> bool {
        match (self, ip) {
            (
                Self::Network {
                    network: IpAddr::V4(network),
                    prefix,
                },
                IpAddr::V4(ip),
            ) => {
                let mask = ipv4_mask(prefix);
                (u32::from(ip) & mask) == (u32::from(network) & mask)
            }
            (
                Self::Network {
                    network: IpAddr::V6(network),
                    prefix,
                },
                IpAddr::V6(ip),
            ) => {
                let mask = ipv6_mask(prefix);
                (u128::from(ip) & mask) == (u128::from(network) & mask)
            }
            _ => false,
        }
    }
}

/// Returns a prefix mask for an IPv4 address; `prefix` is validated elsewhere.
fn ipv4_mask(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    }
}

/// Returns a prefix mask for an IPv6 address; `prefix` is validated elsewhere.
fn ipv6_mask(prefix: u8) -> u128 {
    if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    }
}

/// PHP-facing connection facts derived from the socket peer and trusted headers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConnectionFacts {
    /// `$_SERVER['REMOTE_ADDR']`.
    pub(crate) remote_addr: String,
    /// `$_SERVER['REMOTE_PORT']`.
    pub(crate) remote_port: u16,
    /// `$_SERVER['REQUEST_SCHEME']`.
    pub(crate) request_scheme: String,
    /// Whether `$_SERVER['HTTPS']` must be set to `on`.
    pub(crate) https: bool,
    /// `$_SERVER['SERVER_PORT']`.
    pub(crate) server_port: u16,
}

impl ConnectionFacts {
    /// Returns the socket-peer facts used when no header is trusted.
    fn from_peer(peer: &PeerInfo) -> Self {
        Self {
            remote_addr: peer.remote_addr(),
            remote_port: peer.remote_port(),
            request_scheme: "http".to_string(),
            https: false,
            server_port: peer.server_port(),
        }
    }
}

/// Applies trusted forwarded headers to the socket-peer facts.
///
/// Headers from an untrusted peer are ignored entirely. A malformed forwarded
/// chain also falls back to the socket peer instead of guessing a client.
pub(crate) fn apply(
    peer: &PeerInfo,
    headers: &[(String, String)],
    trusted: &TrustedProxies,
) -> ConnectionFacts {
    let mut facts = ConnectionFacts::from_peer(peer);
    if !trusted.is_trusted(peer) {
        return facts;
    }
    if let Some(value) = header(headers, "forwarded") {
        if let Some(forwarded) = ForwardedHeader::parse(value) {
            forwarded.apply(&mut facts, peer, trusted);
            return facts;
        }
    }
    if let Some(value) = header(headers, "x-forwarded-for") {
        let Some(nodes) = parse_xff(value) else {
            return ConnectionFacts::from_peer(peer);
        };
        if let Some(node) = resolve_chain(&nodes, trusted, peer.remote_ip()) {
            facts.remote_addr = node.ip.to_string();
            if let Some(port) = node.port {
                facts.remote_port = port;
            }
        }
    }
    let scheme_forwarded = header(headers, "x-forwarded-proto")
        .and_then(first_token)
        .and_then(normalize_scheme)
        .map(|scheme| {
            facts.request_scheme = scheme.to_string();
            facts.https = scheme == "https";
        })
        .is_some();
    let host_port = header(headers, "x-forwarded-host").and_then(host_port);
    let forwarded_port = header(headers, "x-forwarded-port").and_then(parse_port);
    if let Some(port) = forwarded_port.or(host_port) {
        facts.server_port = port;
    } else if scheme_forwarded {
        if let Some(port) = default_port(&facts.request_scheme) {
            facts.server_port = port;
        }
    }
    facts
}

/// One parsed `Forwarded` header.
struct ForwardedHeader {
    /// `for=` chain in header order (client first).
    nodes: Vec<ForwardedNode>,
    /// Client-facing protocol from the first element.
    scheme: Option<&'static str>,
    /// Port from the first element's `host=` parameter.
    host_port: Option<u16>,
}

impl ForwardedHeader {
    /// Parses a well-formed RFC 7239 header, or returns `None` to fall back.
    fn parse(value: &str) -> Option<Self> {
        let mut nodes = Vec::new();
        let mut scheme = None;
        let mut first_host_port = None;
        let mut saw_element = false;
        for (index, element) in split_quoted(value, ',').into_iter().enumerate() {
            if element.is_empty() {
                continue;
            }
            saw_element = true;
            let mut node = None;
            let mut element_scheme = None;
            let mut element_host_port = None;
            for pair in split_quoted(element, ';') {
                if pair.is_empty() {
                    continue;
                }
                let (key, value) = pair.split_once('=')?;
                let key = key.trim().to_ascii_lowercase();
                let value = unquote(value);
                match key.as_str() {
                    "for" => node = Some(parse_forwarded_node(&value)?),
                    "proto" => {
                        element_scheme = Some(normalize_scheme(&value)?);
                    }
                    "host" => element_host_port = host_port(&value),
                    _ => {}
                }
            }
            if let Some(node) = node {
                nodes.push(node);
            }
            if index == 0 {
                scheme = element_scheme;
                first_host_port = element_host_port;
            }
        }
        saw_element.then_some(Self {
            nodes,
            scheme,
            host_port: first_host_port,
        })
    }

    /// Applies this header's facts to `facts`.
    fn apply(&self, facts: &mut ConnectionFacts, peer: &PeerInfo, trusted: &TrustedProxies) {
        if let Some(node) = resolve_chain(&self.nodes, trusted, peer.remote_ip()) {
            facts.remote_addr = node.ip.to_string();
            if let Some(port) = node.port {
                facts.remote_port = port;
            }
        }
        if let Some(scheme) = self.scheme {
            facts.request_scheme = scheme.to_string();
            facts.https = scheme == "https";
        }
        if let Some(port) = self.host_port {
            facts.server_port = port;
        } else if self.scheme.is_some() {
            if let Some(port) = default_port(&facts.request_scheme) {
                facts.server_port = port;
            }
        }
    }
}

/// One address in a forwarded chain, with an optional source port.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ForwardedNode {
    /// The address.
    ip: IpAddr,
    /// The source port when the header carried one.
    port: Option<u16>,
}

/// Parses an `X-Forwarded-For` value into a client-first chain.
fn parse_xff(value: &str) -> Option<Vec<ForwardedNode>> {
    let mut nodes = Vec::new();
    for entry in value.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            return None;
        }
        let ip = entry.parse::<IpAddr>().ok()?;
        nodes.push(ForwardedNode { ip, port: None });
    }
    Some(nodes)
}

/// Parses one `for=` identifier, including quoted and bracketed IPv6 forms.
fn parse_forwarded_node(value: &str) -> Option<ForwardedNode> {
    if value.eq_ignore_ascii_case("unknown") || value.starts_with('_') {
        return None;
    }
    if let Some(rest) = value.strip_prefix('[') {
        let (address, rest) = rest.split_once(']')?;
        let ip = address.parse::<Ipv6Addr>().ok()?;
        let port = rest.strip_prefix(':').and_then(parse_port);
        return Some(ForwardedNode {
            ip: IpAddr::V6(ip),
            port,
        });
    }
    if let Ok(ip) = value.parse::<IpAddr>() {
        return Some(ForwardedNode { ip, port: None });
    }
    let (address, port) = value.rsplit_once(':')?;
    let port = parse_port(port)?;
    let ip = address.parse::<IpAddr>().ok()?;
    Some(ForwardedNode {
        ip,
        port: Some(port),
    })
}

/// Returns the rightmost chain entry that is not a trusted proxy.
///
/// When every header hop is trusted, the socket peer is kept instead of
/// inventing a client. `fallback` is `None` for a Unix-domain peer.
fn resolve_chain(
    nodes: &[ForwardedNode],
    trusted: &TrustedProxies,
    fallback: Option<IpAddr>,
) -> Option<ForwardedNode> {
    for node in nodes.iter().rev() {
        if !trusted.is_trusted_ip(node.ip) {
            return Some(*node);
        }
    }
    fallback.map(|ip| ForwardedNode { ip, port: None })
}

/// Finds one header value case-insensitively.
fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// Returns the first non-empty comma-separated token.
fn first_token(value: &str) -> Option<&str> {
    value
        .split(',')
        .next()
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

/// Normalizes `http`/`https` case-insensitively.
fn normalize_scheme(value: &str) -> Option<&'static str> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("https") {
        Some("https")
    } else if value.eq_ignore_ascii_case("http") {
        Some("http")
    } else {
        None
    }
}

/// Returns the default port for a normalized scheme.
fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "https" => Some(443),
        "http" => Some(80),
        _ => None,
    }
}

/// Parses a non-zero decimal port.
fn parse_port(value: &str) -> Option<u16> {
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
}

/// Extracts the port from a `host=` value, if it carries one.
fn host_port(value: &str) -> Option<u16> {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('[') {
        let (_, rest) = rest.split_once(']')?;
        return rest.strip_prefix(':').and_then(parse_port);
    }
    if value.matches(':').count() > 1 {
        return None;
    }
    let (_, port) = value.rsplit_once(':')?;
    parse_port(port)
}

/// Splits on `separator` while ignoring separators inside quoted strings.
fn split_quoted(input: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (index, byte) in input.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && byte == b'\\' {
            escaped = true;
            continue;
        }
        if byte == b'"' {
            quoted = !quoted;
            continue;
        }
        if byte == separator as u8 && !quoted {
            parts.push(input[start..index].trim());
            start = index + 1;
        }
    }
    parts.push(input[start..].trim());
    parts
}

/// Removes RFC 7239 quoting and backslash escapes from one parameter value.
fn unquote(value: &str) -> Cow<'_, str> {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        let inner = &value[1..value.len() - 1];
        let mut output = String::with_capacity(inner.len());
        let mut escaped = false;
        for character in inner.chars() {
            if escaped {
                output.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else {
                output.push(character);
            }
        }
        return Cow::Owned(output);
    }
    Cow::Borrowed(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    /// Builds a TCP peer with the given remote IP and a fixed server port.
    fn peer(ip: &str) -> PeerInfo {
        let remote = SocketAddr::new(ip.parse::<std::net::IpAddr>().unwrap(), 50000);
        let server = "127.0.0.1:8080".parse::<SocketAddr>().unwrap();
        PeerInfo::for_test(Some(remote), Some(server))
    }

    /// Builds a Unix-domain peer.
    fn unix_peer() -> PeerInfo {
        PeerInfo::for_test(None, None)
    }

    /// Wraps one header pair.
    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    /// Parses a trusted-proxy flag value for the tests.
    fn trusted(value: &str) -> TrustedProxies {
        TrustedProxies::parse_values(&[value.to_string()]).expect("trusted proxy must parse")
    }

    /// Verifies IPv4, IPv6, CIDR, and the Unix token parse.
    #[test]
    fn trusted_proxies_parse_ipv4_ipv6_cidr_and_unix() {
        let set = trusted("127.0.0.1/32, 10.0.0.0/8, 2001:db8::/32, unix");
        assert!(set.is_trusted(&peer("127.0.0.1")));
        assert!(set.is_trusted(&peer("10.1.2.3")));
        assert!(set.is_trusted(&peer("2001:db8::1")));
        assert!(!set.is_trusted(&peer("192.0.2.1")));
        assert!(set.is_trusted(&unix_peer()));
    }

    /// Verifies malformed flag entries are rejected.
    #[test]
    fn trusted_proxies_reject_malformed_entries() {
        assert!(TrustedProxies::parse_values(&["not-an-ip".to_string()]).is_err());
        assert!(TrustedProxies::parse_values(&["127.0.0.1/33".to_string()]).is_err());
        assert!(TrustedProxies::parse_values(&["".to_string()]).is_err());
        assert!(TrustedProxies::parse_values(&["127.0.0.1,".to_string()]).is_err());
    }

    /// A trusted single hop replaces the peer address and the scheme.
    #[test]
    fn forwarded_single_hop_replaces_peer() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("X-Forwarded-For", "203.0.113.9"),
                ("X-Forwarded-Proto", "https"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
        assert_eq!(facts.request_scheme, "https");
        assert!(facts.https);
        assert_eq!(facts.server_port, 443);
    }

    /// A multi-hop chain skips trusted intermediates and picks the client.
    #[test]
    fn forwarded_multi_hop_skips_trusted_hops() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[(
                "X-Forwarded-For",
                "203.0.113.9, 10.0.0.5, 10.0.0.6",
            )]),
            &trusted("127.0.0.1/32,10.0.0.0/8"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
    }

    /// An untrusted peer ignores every forwarded header.
    #[test]
    fn untrusted_peer_ignores_headers() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("X-Forwarded-For", "203.0.113.9"),
                ("X-Forwarded-Proto", "https"),
            ]),
            &trusted("10.0.0.0/8"),
        );
        assert_eq!(facts.remote_addr, "127.0.0.1");
        assert_eq!(facts.request_scheme, "http");
        assert!(!facts.https);
        assert_eq!(facts.server_port, 8080);
    }

    /// RFC 7239 `Forwarded` takes precedence over the X-Forwarded headers.
    #[test]
    fn forwarded_beats_x_forwarded_headers() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("Forwarded", "for=203.0.113.9;proto=https;host=example.com:8443"),
                ("X-Forwarded-For", "198.51.100.1"),
                ("X-Forwarded-Proto", "http"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
        assert_eq!(facts.request_scheme, "https");
        assert!(facts.https);
        assert_eq!(facts.server_port, 8443);
    }

    /// Quoted and bracketed IPv6 `for=` values preserve the source port.
    #[test]
    fn forwarded_accepts_quoted_bracketed_ipv6() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("Forwarded", "for=\"[2001:db8::1]:1234\";proto=https")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "2001:db8::1");
        assert_eq!(facts.remote_port, 1234);
        assert!(facts.https);
        assert_eq!(facts.server_port, 443);
    }

    /// Obfuscated identifiers fall back to the socket peer.
    #[test]
    fn forwarded_obfuscated_identifier_falls_back() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("Forwarded", "for=unknown;proto=https")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "127.0.0.1");
        assert_eq!(facts.request_scheme, "http");
        assert!(!facts.https);
    }

    /// `X-Forwarded-Port` wins over the scheme default.
    #[test]
    fn x_forwarded_port_wins_over_scheme_default() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("X-Forwarded-For", "203.0.113.9"),
                ("X-Forwarded-Proto", "https"),
                ("X-Forwarded-Port", "9443"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
        assert_eq!(facts.server_port, 9443);
        assert!(facts.https);
    }

    /// HTTP leaves `HTTPS` absent and does not claim port 443.
    #[test]
    fn http_scheme_has_no_https_and_uses_port_80() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("X-Forwarded-Proto", "http")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.request_scheme, "http");
        assert!(!facts.https);
        assert_eq!(facts.server_port, 80);
    }

    /// The `unix` token trusts a Unix peer; an IP entry does not.
    #[test]
    fn unix_token_trusts_only_unix_peers() {
        let facts = apply(
            &unix_peer(),
            &headers(&[("X-Forwarded-For", "203.0.113.9")]),
            &trusted("unix"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");

        let ignored = apply(
            &unix_peer(),
            &headers(&[("X-Forwarded-For", "203.0.113.9")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(ignored.remote_addr, "");
    }
}
