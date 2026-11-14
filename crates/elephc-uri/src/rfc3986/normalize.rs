//! Purpose:
//! RFC 3986 syntax normalization for `Uri\Rfc3986\Uri`: percent-encoding repair, case
//! normalization, dot-segment removal, and uriparser's canonical IPv6 serialization.
//!
//! Called from:
//! - `crate::rfc3986::RfcUri::component` and `RfcUri::to_string`.
//!
//! Key details:
//! - Dot-segment removal is a direct model of uriparser's `RemoveDotSegmentsEx`, including
//!   its preservation of `./` and leading `..` in relative references.
//! - The normalized host keeps the caller's IPv6 compression (only lowercasing it), while
//!   serialization expands the parsed address to eight zero-padded groups, as uriparser does.

use super::parse::{is_unreserved, port_to_i64};
use super::{Host, HostKind, RfcUri};

/// Returns `value` unchanged, or its RFC 3986 percent-encoding normalization.
pub fn normalize_component(value: &str, normalized: bool) -> String {
    if normalized {
        normalize_percent(value)
    } else {
        value.to_string()
    }
}

/// Returns the normalized host text (without brackets) for one parsed host.
pub fn normalize_host(host: &Host) -> String {
    match host.kind {
        HostKind::RegName => lowercase_except_percent(&normalize_percent(&host.text)),
        HostKind::Ipv6 | HostKind::IpvFuture => host.text.to_ascii_lowercase(),
    }
}

/// Lowercases ASCII letters without touching the hex digits of a percent escape.
///
/// uriparser case-normalizes the host *before* repairing percent escapes, so `%E4` keeps its
/// uppercase hex while `EXAMPLE.COM` becomes `example.com`.
pub fn lowercase_except_percent(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            out.push('%');
            out.push(char::from(upper_hex_value(bytes[index + 1])));
            out.push(char::from(upper_hex_value(bytes[index + 2])));
            index += 3;
            continue;
        }
        out.push((bytes[index] as char).to_ascii_lowercase());
        index += 1;
    }
    out
}

/// Returns the uppercase ASCII form of a hexadecimal digit byte.
fn upper_hex_value(byte: u8) -> u8 {
    match byte {
        b'a'..=b'f' => byte.to_ascii_uppercase(),
        other => other,
    }
}

/// Returns the normalized path for `uri`.
pub fn normalize_path(uri: &RfcUri) -> String {
    if uri.path.is_empty() {
        return String::new();
    }
    let path = normalize_percent(&uri.path);
    let has_host = uri.authority.is_some();
    let absolute_path = path.starts_with('/') && !has_host;
    let relative = uri.scheme.is_none() && !absolute_path;
    remove_dot_segments(&path, absolute_path, relative, has_host)
}

/// Returns the port text in the requested mode.
pub fn normalize_port(port: &str, normalized: bool) -> String {
    if !normalized || port.is_empty() {
        return port.to_string();
    }
    match port_to_i64(port) {
        Ok(value) => value.to_string(),
        Err(()) => port.to_string(),
    }
}

/// Expands a parsed IPv6 address to uriparser's eight-group serialized form.
pub fn serialize_ipv6(text: &str) -> String {
    match text.parse::<std::net::Ipv6Addr>() {
        Ok(address) => address
            .segments()
            .iter()
            .map(|segment| format!("{segment:04x}"))
            .collect::<Vec<_>>()
            .join(":"),
        Err(_) => text.to_ascii_lowercase(),
    }
}

/// Repairs percent escapes: unreserved octets are decoded, other escapes are uppercased.
fn normalize_percent(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = hex_value(bytes[index + 1]);
            let low = hex_value(bytes[index + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                let decoded = (high << 4) | low;
                if is_unreserved(decoded) {
                    out.push(char::from(decoded));
                } else {
                    out.push('%');
                    out.push(char::from(upper_hex(high)));
                    out.push(char::from(upper_hex(low)));
                }
                index += 3;
                continue;
            }
        }
        out.push(char::from(bytes[index]));
        index += 1;
    }
    out
}

/// Returns the numeric value of one hexadecimal digit.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Returns the uppercase ASCII form of a numeric hexadecimal digit.
fn upper_hex(value: u8) -> u8 {
    match value {
        0..=9 => b'0' + value,
        _ => b'A' + value - 10,
    }
}

/// One node in the path-segment list uriparser manipulates.
struct Segment {
    /// Segment text.
    text: String,
    /// Previous node index.
    prev: Option<usize>,
    /// Next node index.
    next: Option<usize>,
}

/// Removes dot segments exactly as uriparser's `RemoveDotSegmentsEx` does.
///
/// `absolute_path` is uriparser's flag for a leading slash on a URI without a host.
/// `relative` enables its underflow-preserving behavior for relative references.
/// `has_host` preserves the `/.//` and trailing-slash cases that would otherwise change
/// how the path is re-parsed.
pub(crate) fn remove_dot_segments(
    path: &str,
    absolute_path: bool,
    relative: bool,
    has_host: bool,
) -> String {
    // RFC 3986 §5.3 recomposes an authority-only reference with an EMPTY path as
    // `scheme://authority`, not `scheme://authority/`. Splitting the empty string would
    // otherwise yield one empty segment and add a trailing solidus.
    if path.is_empty() {
        return String::new();
    }
    let body = path.strip_prefix('/').unwrap_or(path);
    let mut nodes: Vec<Segment> = Vec::new();
    let mut head: Option<usize> = None;
    let mut tail: Option<usize> = None;
    let mut previous: Option<usize> = None;
    for text in body.split('/') {
        let index = nodes.len();
        nodes.push(Segment {
            text: text.to_string(),
            prev: previous,
            next: None,
        });
        if let Some(previous) = previous {
            nodes[previous].next = Some(index);
        } else {
            head = Some(index);
        }
        tail = Some(index);
        previous = Some(index);
    }

    if head.is_none() {
        return if absolute_path { "/".to_string() } else { String::new() };
    }

    let mut walker = head;
    while let Some(index) = walker {
        let next_backup = nodes[index].next;
        let remove = if nodes[index].text == "." {
            let mut remove = true;
            if Some(index) == head && nodes[index].next.is_some() {
                let next = nodes[index].next.unwrap();
                if nodes[next].text.is_empty() && !has_host {
                    remove = false;
                } else if relative && nodes[next].text.contains(':') {
                    remove = false;
                }
            }
            if remove {
                remove_dot(&mut nodes, &mut head, &mut tail, index, has_host);
            }
            remove
        } else if nodes[index].text == ".." {
            let prev = nodes[index].prev;
            let mut remove = true;
            if relative {
                if prev.is_none() {
                    remove = false;
                } else if let Some(prev) = prev {
                    if nodes[prev].text == ".." {
                        remove = false;
                    }
                }
            }
            if remove {
                remove_dot_dot(
                    &mut nodes,
                    &mut head,
                    &mut tail,
                    index,
                    absolute_path,
                );
            }
            remove
        } else {
            false
        };

        walker = if remove { next_backup } else { nodes[index].next };
    }

    if !absolute_path && !has_host {
        if let Some(index) = head {
            if nodes[index].next.is_none() && nodes[index].text.is_empty() {
                head = None;
                tail = None;
            }
        }
    }

    let mut segments = Vec::new();
    let mut cursor = head;
    while let Some(index) = cursor {
        segments.push(nodes[index].text.as_str());
        cursor = nodes[index].next;
    }
    let mut out = String::new();
    if absolute_path || (has_host && !segments.is_empty()) {
        out.push('/');
    }
    out.push_str(&segments.join("/"));
    if head.is_none() && tail.is_some() {
        out.push('/');
    }
    out
}

/// Removes one `.` segment, mirroring uriparser's single-dot branch.
fn remove_dot(
    nodes: &mut Vec<Segment>,
    head: &mut Option<usize>,
    tail: &mut Option<usize>,
    index: usize,
    has_host: bool,
) {
    match nodes[index].next {
        Some(next) => {
            nodes[next].prev = nodes[index].prev;
            match nodes[index].prev {
                Some(prev) => nodes[prev].next = Some(next),
                None => *head = Some(next),
            }
        }
        None => match nodes[index].prev {
            Some(_) => nodes[index].text.clear(),
            None => {
                if has_host {
                    nodes[index].text.clear();
                } else {
                    *head = None;
                    *tail = None;
                }
            }
        },
    }
}

/// Removes one `..` segment and its predecessor, mirroring uriparser's double-dot branch.
fn remove_dot_dot(
    nodes: &mut Vec<Segment>,
    head: &mut Option<usize>,
    tail: &mut Option<usize>,
    index: usize,
    absolute_path: bool,
) {
    let prev = nodes[index].prev;
    match prev {
        Some(prev) => {
            let prev_prev = nodes[prev].prev;
            match prev_prev {
                Some(prev_prev) => {
                    nodes[prev_prev].next = nodes[index].next;
                    if let Some(next) = nodes[index].next {
                        nodes[next].prev = Some(prev_prev);
                    } else {
                        let segment = nodes.len();
                        nodes.push(Segment {
                            text: String::new(),
                            prev: Some(prev_prev),
                            next: None,
                        });
                        nodes[prev_prev].next = Some(segment);
                        *tail = Some(segment);
                    }
                    detach(nodes, prev);
                    detach(nodes, index);
                }
                None => {
                    match nodes[index].next {
                        Some(next) => {
                            nodes[next].prev = None;
                            *head = Some(next);
                            detach(nodes, index);
                        }
                        None => {
                            nodes[index].text.clear();
                            *head = Some(index);
                            *tail = Some(index);
                        }
                    }
                    detach(nodes, prev);
                }
            }
        }
        None => match nodes[index].next {
            Some(next) => {
                nodes[next].prev = None;
                *head = Some(next);
                detach(nodes, index);
            }
            None => {
                if absolute_path {
                    *head = None;
                    *tail = None;
                } else {
                    nodes[index].text.clear();
                }
            }
        },
    }
}

/// Marks one node as detached from the live segment list.
fn detach(nodes: &mut Vec<Segment>, index: usize) {
    nodes[index].prev = None;
    nodes[index].next = None;
}
