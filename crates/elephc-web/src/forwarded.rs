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
//! - A present RFC 7239 `Forwarded` header is authoritative: `X-Forwarded-*`
//!   is only consulted when no `Forwarded` header is present. Malformed or
//!   obfuscated identifiers keep the socket peer instead of letting a
//!   client-supplied `X-Forwarded-For` win.
//! - The chain is walked right-to-left, so only trusted hops are skipped.
//! - Repeated scalar headers use the last line and its rightmost token, which
//!   is the value added by the proxy closest to the origin.

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

    /// Returns whether no trusted proxy is configured.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
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
        let normalized = normalize_ip(ip);
        self.entries
            .iter()
            .any(|entry| entry.matches(ip) || entry.matches(normalized))
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
    pub(crate) fn from_peer(peer: &PeerInfo) -> Self {
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
/// Headers from an untrusted peer are ignored entirely. A present `Forwarded`
/// header is authoritative; an incomplete chain keeps the socket peer.
pub(crate) fn apply(
    peer: &PeerInfo,
    headers: &[(String, String)],
    trusted: &TrustedProxies,
) -> ConnectionFacts {
    let mut facts = ConnectionFacts::from_peer(peer);
    if !trusted.is_trusted(peer) {
        return facts;
    }
    let forwarded_values: Vec<&str> = header_values(headers, "forwarded")
        .filter(|value| !value.trim().is_empty())
        .collect();
    if !forwarded_values.is_empty() {
        if let Some(forwarded) = ForwardedHeader::parse(&forwarded_values) {
            forwarded.apply(&mut facts, trusted);
            // Do not consult the client-supplied X-Forwarded family once the
            // trusted hop has committed to RFC 7239.
            return facts;
        }
    }
    if let Some(value) = header_joined(headers, "x-forwarded-for") {
        // A bad entry on the client side must not discard the address the
        // trusted proxy appended on the right. Walk right-to-left and stop at
        // the first non-parsing entry only if it is closer to the origin.
        if let Some(node) = resolve_xff(&parse_xff(&value), trusted) {
            facts.remote_addr = node.ip.to_string();
            facts.remote_port = node.port.unwrap_or(0);
        }
    }
    let scheme_forwarded = header_last(headers, "x-forwarded-proto")
        .and_then(last_token)
        .and_then(normalize_scheme)
        .map(|scheme| {
            facts.request_scheme = scheme.to_string();
            facts.https = scheme == "https";
        })
        .is_some();
    // `X-Forwarded-Host` is not a port authority. Only an explicit
    // X-Forwarded-Port or an RFC 7239 `host=` parameter may set SERVER_PORT.
    let forwarded_port = header_last(headers, "x-forwarded-port")
        .and_then(last_token)
        .and_then(parse_port);
    if let Some(port) = forwarded_port {
        facts.server_port = port;
    } else if scheme_forwarded {
        if let Some(port) = default_port(&facts.request_scheme) {
            facts.server_port = port;
        }
    }
    facts
}

/// One element of an RFC 7239 `Forwarded` header.
struct ForwardedElement {
    /// The `for=` address, or `None` for unknown/obfuscated/malformed entries.
    node: Option<ForwardedNode>,
    /// The element's `proto=` value, when it parsed.
    scheme: Option<&'static str>,
    /// The element's `host=` port, when present and valid.
    host_port: Option<u16>,
    /// Whether the element carried a `for=` parameter (even an invalid one).
    for_present: bool,
}

impl ForwardedElement {
    /// Returns an element that carries no trusted parameter and stops the walk.
    fn invalid() -> Self {
        Self {
            node: None,
            scheme: None,
            host_port: None,
            for_present: false,
        }
    }
}

/// Applies one element's `proto` and `host` without touching the address.
fn apply_scheme_and_host(facts: &mut ConnectionFacts, element: &ForwardedElement) {
    if let Some(scheme) = element.scheme {
        facts.request_scheme = scheme.to_string();
        facts.https = scheme == "https";
    }
    if let Some(port) = element.host_port {
        facts.server_port = port;
    } else if element.scheme.is_some() {
        if let Some(port) = default_port(&facts.request_scheme) {
            facts.server_port = port;
        }
    }
}

/// One parsed `Forwarded` header, retaining every element in wire order.
struct ForwardedHeader {
    /// Header elements in wire order (client first).
    elements: Vec<ForwardedElement>,
}

impl ForwardedHeader {
    /// Parses every parseable element instead of rejecting the whole header.
    ///
    /// Each header line is split on its own: an unbalanced quote in a
    /// client-supplied line must not swallow the element appended by the
    /// trusted proxy on the next line. An unbalanced line or element becomes
    /// an invalid sentinel, which ends the right-to-left walk without trusting
    /// any parameter from it.
    fn parse(values: &[&str]) -> Option<Self> {
        let mut elements = Vec::new();
        let mut has_meaningful = false;
        for value in values {
            let Some(line_elements) = split_quoted(value, ',') else {
                elements.push(ForwardedElement::invalid());
                continue;
            };
            for element in line_elements {
                if element.trim().is_empty() {
                    continue;
                }
                let Some(pairs) = split_quoted(element, ';') else {
                    elements.push(ForwardedElement::invalid());
                    continue;
                };
                let mut node = None;
                let mut scheme = None;
                let mut host_port_value = None;
                let mut for_present = false;
                for pair in pairs {
                    if pair.trim().is_empty() {
                        continue;
                    }
                    let Some((key, value)) = pair.split_once('=') else {
                        continue;
                    };
                    let key = key.trim().to_ascii_lowercase();
                    let value = unquote(value.trim());
                    match key.as_str() {
                        "for" => {
                            for_present = true;
                            node = parse_forwarded_node(&value);
                        }
                        "proto" => scheme = normalize_scheme(&value),
                        "host" => host_port_value = host_port(&value),
                        _ => {}
                    }
                }
                if node.is_some() || scheme.is_some() || host_port_value.is_some() || for_present {
                    has_meaningful = true;
                }
                elements.push(ForwardedElement {
                    node,
                    scheme,
                    host_port: host_port_value,
                    for_present,
                });
            }
        }
        has_meaningful.then_some(Self { elements })
    }

    /// Applies the closest untrusted element, including its proto and host.
    ///
    /// An unknown, obfuscated, or malformed entry ends the walk and leaves the
    /// socket peer in place.
    fn apply(&self, facts: &mut ConnectionFacts, trusted: &TrustedProxies) {
        let mut closest_trusted = None;
        for element in self.elements.iter().rev() {
            let Some(node) = element.node else {
                if !element.for_present {
                    // A `proto=`/`host=`-only element is still the trusted
                    // hop's statement about the client-facing connection.
                    apply_scheme_and_host(facts, element);
                }
                // Unknown, obfuscated, or malformed `for=` ends the walk.
                return;
            };
            if trusted.is_trusted_ip(node.ip) {
                if closest_trusted.is_none() {
                    closest_trusted = Some(element);
                }
                continue;
            }
            facts.remote_addr = normalize_ip(node.ip).to_string();
            facts.remote_port = node.port.unwrap_or(0);
            apply_scheme_and_host(facts, element);
            return;
        }
        // The whole chain is trusted: the address stays the socket peer, but
        // the closest proxy may still publish the client-facing scheme/host.
        if let Some(element) = closest_trusted {
            apply_scheme_and_host(facts, element);
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
///
/// Empty entries are skipped; a non-empty token that does not parse becomes
/// `None`, which ends the right-to-left walk at that point.
fn parse_xff(value: &str) -> Vec<Option<ForwardedNode>> {
    value
        .split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                None
            } else {
                Some(parse_forwarded_node(entry))
            }
        })
        .collect()
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
/// A non-parsing entry stops the walk; entries to its left are client-supplied
/// and untrustworthy. When every parsed hop is trusted, the socket peer stays
/// authoritative.
fn resolve_xff(
    nodes: &[Option<ForwardedNode>],
    trusted: &TrustedProxies,
) -> Option<ForwardedNode> {
    for node in nodes.iter().rev() {
        match node {
            Some(node) if !trusted.is_trusted_ip(node.ip) => {
                return Some(ForwardedNode {
                    ip: normalize_ip(node.ip),
                    port: node.port,
                });
            }
            Some(_) => {}
            None => return None,
        }
    }
    None
}

/// Returns every value for `name`, in wire order, case-insensitively.
fn header_values<'a>(
    headers: &'a [(String, String)],
    name: &'a str,
) -> impl Iterator<Item = &'a str> + 'a {
    headers
        .iter()
        .filter(move |(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// Joins every occurrence of a comma-list header, as HTTP semantics require.
fn header_joined<'a>(headers: &'a [(String, String)], name: &'a str) -> Option<String> {
    let mut values = header_values(headers, name).filter(|value| !value.trim().is_empty());
    let first = values.next()?;
    let mut joined = String::from(first);
    for value in values {
        joined.push_str(", ");
        joined.push_str(value);
    }
    Some(joined)
}

/// Returns the last occurrence of a scalar forwarded header.
///
/// The last line is the one added by the proxy closest to the origin; using
/// the first line would let a client-supplied line win when an edge appends a
/// separate header line instead of replacing the value.
fn header_last<'a>(headers: &'a [(String, String)], name: &'a str) -> Option<&'a str> {
    header_values(headers, name)
        .filter(|value| !value.trim().is_empty())
        .last()
}

/// Normalizes IPv4-mapped IPv6 addresses to their IPv4 form.
fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(address) => address
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(address)),
        IpAddr::V4(_) => ip,
    }
}

/// Returns the rightmost non-empty comma-separated token.
///
/// A proxy that appends to an existing scalar header puts its own value last;
/// choosing the first token would let a client-supplied value win.
fn last_token(value: &str) -> Option<&str> {
    value
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .last()
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
///
/// Returns `None` when the final quote is unbalanced. Callers must not trust
/// any element from an unbalanced slice.
fn split_quoted(input: &str, separator: char) -> Option<Vec<&str>> {
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
    if quoted || escaped {
        return None;
    }
    parts.push(input[start..].trim());
    Some(parts)
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

    /// Bare IPv6 entries (the nginx `$proxy_add_x_forwarded_for` form) parse.
    #[test]
    fn xff_accepts_bare_ipv6() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("X-Forwarded-For", "2001:db8::1")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "2001:db8::1");
    }

    /// A proto/host-only `Forwarded` element still supplies the scheme.
    #[test]
    fn forwarded_proto_without_for_applies() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("Forwarded", "proto=https")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "127.0.0.1");
        assert_eq!(facts.request_scheme, "https");
        assert!(facts.https);
        assert_eq!(facts.server_port, 443);
    }

    /// An unbalanced client quote must not swallow the trusted hop's line.
    #[test]
    fn forwarded_unbalanced_quote_does_not_swallow_appended_line() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("Forwarded", "for=1.2.3.4;proto=https;x=\""),
                ("Forwarded", "for=203.0.113.9"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
        assert_eq!(facts.request_scheme, "http");
        assert!(!facts.https);
        assert_eq!(facts.server_port, 8080);
    }

    /// An unbalanced quote inside one line invalidates that whole line.
    #[test]
    fn forwarded_unbalanced_quote_single_line_stops_walk() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[(
                "Forwarded",
                "for=1.2.3.4;proto=https;x=\", for=203.0.113.9",
            )]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "127.0.0.1");
        assert_eq!(facts.request_scheme, "http");
        assert!(!facts.https);
        assert_eq!(facts.server_port, 8080);
    }

    /// A Forwarded header with no meaningful parameter does not hide XFF.
    #[test]
    fn forwarded_garbage_falls_back_to_xff() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("Forwarded", "x"),
                ("X-Forwarded-For", "9.9.9.9"),
                ("X-Forwarded-Proto", "https"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "9.9.9.9");
        assert_eq!(facts.request_scheme, "https");
        assert!(facts.https);
        assert_eq!(facts.server_port, 443);
    }

    /// An all-trusted chain keeps the peer address but uses the closest
    /// element's proto/host.
    #[test]
    fn forwarded_all_trusted_chain_applies_closest_proto() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("Forwarded", "for=10.0.0.5;proto=https;host=example.com:8443")]),
            &trusted("127.0.0.1/32,10.0.0.0/8"),
        );
        assert_eq!(facts.remote_addr, "127.0.0.1");
        assert_eq!(facts.request_scheme, "https");
        assert_eq!(facts.server_port, 8443);
    }

    /// A bad entry on the client side must not hide the address the trusted
    /// proxy appended on the right.
    #[test]
    fn xff_bad_left_token_keeps_the_appended_client() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("X-Forwarded-For", "x, 198.51.100.7")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "198.51.100.7");
    }

    /// A load-balancer `ip:port` entry parses and updates REMOTE_PORT.
    #[test]
    fn xff_accepts_address_with_port() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("X-Forwarded-For", "198.51.100.7:4711")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "198.51.100.7");
        assert_eq!(facts.remote_port, 4711);
    }

    /// A forwarded address without a port does not keep the proxy's port.
    #[test]
    fn xff_without_port_clears_the_proxy_port() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("X-Forwarded-For", "198.51.100.7")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "198.51.100.7");
        assert_eq!(facts.remote_port, 0);
    }

    /// The invalid entry closest to the origin ends the walk and keeps the peer.
    #[test]
    fn xff_invalid_rightmost_keeps_the_peer() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("X-Forwarded-For", "x")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "127.0.0.1");
    }

    /// Proto and host come from the element whose `for=` was selected.
    #[test]
    fn forwarded_uses_the_chosen_element_proto_and_host() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[(
                "Forwarded",
                "for=1.1.1.1;proto=https;host=\"evil:8443\", for=203.0.113.9;proto=http",
            )]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
        assert_eq!(facts.request_scheme, "http");
        assert!(!facts.https);
        assert_eq!(facts.server_port, 80);
    }

    /// The chosen element's host port wins over the client-side element.
    #[test]
    fn forwarded_host_port_comes_from_the_chosen_element() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[(
                "Forwarded",
                "for=1.1.1.1;proto=https;host=\"evil:8443\", for=203.0.113.9;proto=http;host=\"good:8080\"",
            )]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
        assert_eq!(facts.server_port, 8080);
    }

    /// A malformed element left of the chosen client does not invalidate it.
    #[test]
    fn forwarded_garbage_left_of_the_client_is_ignored() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("Forwarded", "garbage, for=203.0.113.9")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
    }

    /// An unknown identifier closest to the origin ends the walk.
    #[test]
    fn forwarded_unknown_rightmost_ends_the_walk() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("Forwarded", "for=203.0.113.9, for=unknown")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "127.0.0.1");
        assert_eq!(facts.request_scheme, "http");
    }

    /// A trusted chain followed by an invalid client entry falls back to the peer.
    #[test]
    fn xff_trusted_chain_with_invalid_client_falls_back() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("X-Forwarded-For", "x, 10.0.0.5, 10.0.0.6")]),
            &trusted("127.0.0.1/32,10.0.0.0/8"),
        );
        assert_eq!(facts.remote_addr, "127.0.0.1");
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

    /// A present Forwarded header blocks a client-supplied X-Forwarded family.
    #[test]
    fn forwarded_presence_blocks_x_forwarded_fallback() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("Forwarded", "for=unknown;proto=https"),
                ("X-Forwarded-For", "203.0.113.9"),
                ("X-Forwarded-Proto", "https"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "127.0.0.1");
        assert_eq!(facts.request_scheme, "http");
        assert!(!facts.https);
    }

    /// Repeated comma-list headers are merged in wire order.
    #[test]
    fn duplicate_x_forwarded_for_lines_are_merged() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("X-Forwarded-For", "6.6.6.6"),
                ("X-Forwarded-For", "203.0.113.9"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
    }

    /// Repeated scalar headers use the last line, added by the closest proxy.
    #[test]
    fn duplicate_scalar_headers_use_the_last_line() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("X-Forwarded-Proto", "http"),
                ("X-Forwarded-Proto", "https"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.request_scheme, "https");
        assert!(facts.https);
        assert_eq!(facts.server_port, 443);
    }

    /// A comma-separated scalar uses its rightmost token, not its first.
    #[test]
    fn same_line_scalar_uses_the_rightmost_token() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("X-Forwarded-Proto", "https, http"),
                ("X-Forwarded-Port", "8443, 8080"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.request_scheme, "http");
        assert!(!facts.https);
        assert_eq!(facts.server_port, 8080);
    }

    /// Repeated RFC 7239 fields are merged before the chain walk.
    #[test]
    fn duplicate_forwarded_lines_are_merged() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("Forwarded", "for=203.0.113.9;proto=https"),
                ("Forwarded", "for=10.0.0.5"),
            ]),
            &trusted("127.0.0.1/32,10.0.0.0/8"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
        assert_eq!(facts.request_scheme, "https");
        assert!(facts.https);
    }

    /// Repeated X-Forwarded-Port lines use the last line.
    #[test]
    fn duplicate_x_forwarded_port_uses_the_last_line() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("X-Forwarded-Proto", "https"),
                ("X-Forwarded-Port", "8443"),
                ("X-Forwarded-Port", "9443"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.server_port, 9443);
    }

    /// A malformed XFF keeps the peer address but does not discard a valid proto.
    #[test]
    fn malformed_xff_keeps_valid_proto() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("X-Forwarded-For", "203.0.113.9, unknown"),
                ("X-Forwarded-Proto", "https"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "127.0.0.1");
        assert_eq!(facts.request_scheme, "https");
        assert!(facts.https);
        assert_eq!(facts.server_port, 443);
    }

    /// A leading empty Forwarded element must not hide the first real element's proto.
    #[test]
    fn forwarded_leading_empty_element_preserves_proto() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[("Forwarded", ", for=203.0.113.9;proto=https")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
        assert_eq!(facts.request_scheme, "https");
        assert!(facts.https);
    }

    /// X-Forwarded-Host is not an authority for SERVER_PORT.
    #[test]
    fn x_forwarded_host_does_not_set_server_port() {
        let facts = apply(
            &peer("127.0.0.1"),
            &headers(&[
                ("X-Forwarded-Proto", "https"),
                ("X-Forwarded-Host", "example.com:8443"),
            ]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.server_port, 443);
    }

    /// IPv4-mapped IPv6 peers match an IPv4 trusted network.
    #[test]
    fn ipv4_mapped_peer_matches_ipv4_trusted_network() {
        let facts = apply(
            &peer("::ffff:127.0.0.1"),
            &headers(&[("X-Forwarded-For", "203.0.113.9")]),
            &trusted("127.0.0.1/32"),
        );
        assert_eq!(facts.remote_addr, "203.0.113.9");
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
