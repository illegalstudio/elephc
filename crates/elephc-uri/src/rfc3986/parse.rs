//! Purpose:
//! Strict RFC 3986 reference parser for `Uri\Rfc3986\Uri`, mirroring the grammar and
//! component boundaries enforced by the uriparser library bundled with PHP's ext/uri.
//!
//! Called from:
//! - `crate::rfc3986::parse` and `crate::rfc3986::mutate` for path disambiguation checks.
//!
//! Key details:
//! - Input is binary but only ASCII is accepted; a single non-ASCII byte rejects the
//!   reference, exactly as `new Uri\Rfc3986\Uri("🐘")` does in PHP.
//! - Percent escapes must be complete and are validated in every component.
//! - A colon before the first slash is always a scheme separator; an invalid scheme is a
//!   syntax error rather than a relative path, matching uriparser.

use super::{Authority, Host, HostKind, RfcUri};

/// Parses one RFC 3986 URI reference from raw bytes.
pub fn parse(input: &[u8]) -> Result<RfcUri, String> {
    if !input.is_ascii() {
        return Err(malformed());
    }
    let input = std::str::from_utf8(input).map_err(|_| malformed())?;

    let (before_fragment, fragment) = split_once(input, '#');
    let (before_query, query) = split_once(before_fragment, '?');

    let (scheme, rest) = parse_scheme(before_query)?;
    let (authority, path) = if let Some(after_slashes) = rest.strip_prefix("//") {
        let end = after_slashes.find('/').unwrap_or(after_slashes.len());
        let authority = parse_authority(&after_slashes[..end])?;
        (Some(authority), &after_slashes[end..])
    } else {
        (None, rest)
    };

    validate_path(path, authority.is_some(), scheme.is_some())?;
    if let Some(query) = query {
        if !is_valid_query_or_fragment(query) {
            return Err(malformed());
        }
    }
    if let Some(fragment) = fragment {
        if !is_valid_query_or_fragment(fragment) {
            return Err(malformed());
        }
    }
    if let Some(authority) = &authority {
        if let Some(port) = &authority.port {
            if !port.is_empty() && port_to_i64(port).is_err() {
                return Err("The port is out of range".to_string());
            }
        }
    }

    Ok(RfcUri {
        scheme: scheme.map(str::to_string),
        authority,
        path: path.to_string(),
        query: query.map(str::to_string),
        fragment: fragment.map(str::to_string),
    })
}

/// Splits `input` at the first occurrence of `separator`.
fn split_once(input: &str, separator: char) -> (&str, Option<&str>) {
    match input.find(separator) {
        Some(index) => (&input[..index], Some(&input[index + separator.len_utf8()..])),
        None => (input, None),
    }
}

/// Splits an optional scheme from the rest of the reference.
fn parse_scheme(input: &str) -> Result<(Option<&str>, &str), String> {
    if input.starts_with("//") {
        return Ok((None, input));
    }
    let Some(colon) = input.find(':') else {
        return Ok((None, input));
    };
    let slash = input.find('/').unwrap_or(input.len());
    if colon > slash {
        return Ok((None, input));
    }
    let candidate = &input[..colon];
    if !is_valid_scheme(candidate) {
        return Err(malformed());
    }
    Ok((Some(candidate), &input[colon + 1..]))
}

/// Returns whether `value` matches `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`.
pub(crate) fn is_valid_scheme(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    bytes.all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')
    })
}

/// Parses an authority into userinfo, host, and port.
fn parse_authority(input: &str) -> Result<Authority, String> {
    let (userinfo, host_port) = match input.find('@') {
        Some(index) => (Some(&input[..index]), &input[index + 1..]),
        None => (None, input),
    };
    if let Some(userinfo) = userinfo {
        if !is_valid_userinfo(userinfo) {
            return Err(malformed());
        }
    }

    let (host, port) = if let Some(rest) = host_port.strip_prefix('[') {
        let Some(close) = rest.find(']') else {
            return Err(malformed());
        };
        let inside = &rest[..close];
        let kind = if inside.starts_with('v') || inside.starts_with('V') {
            if !is_valid_ipvfuture(inside) {
                return Err(malformed());
            }
            HostKind::IpvFuture
        } else {
            if inside.parse::<std::net::Ipv6Addr>().is_err() {
                return Err(malformed());
            }
            HostKind::Ipv6
        };
        let after = &rest[close + 1..];
        let port = match after.strip_prefix(':') {
            Some(port) => Some(port),
            None if after.is_empty() => None,
            None => return Err(malformed()),
        };
        (
            Host {
                text: inside.to_string(),
                kind,
            },
            port,
        )
    } else {
        let (host, port) = match host_port.find(':') {
            Some(index) => (&host_port[..index], Some(&host_port[index + 1..])),
            None => (host_port, None),
        };
        if !is_valid_reg_name(host) {
            return Err(malformed());
        }
        (
            Host {
                text: host.to_string(),
                kind: HostKind::RegName,
            },
            port,
        )
    };

    if let Some(port) = port {
        if !port.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(malformed());
        }
    }

    Ok(Authority {
        userinfo: userinfo.map(str::to_string),
        host,
        port: port.map(str::to_string),
    })
}

/// Returns whether `value` is a well-formed userinfo component.
pub(crate) fn is_valid_userinfo(value: &str) -> bool {
    is_valid_component(value, |byte| {
        is_unreserved(byte) || is_sub_delim(byte) || byte == b':'
    })
}

/// Returns whether `value` is a well-formed registered name.
pub(crate) fn is_valid_reg_name(value: &str) -> bool {
    is_valid_component(value, |byte| is_unreserved(byte) || is_sub_delim(byte))
}

/// Returns whether `value` is a well-formed IPvFuture literal (without brackets).
pub(crate) fn is_valid_ipvfuture(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 4 || !matches!(bytes[0], b'v' | b'V') {
        return false;
    }
    let Some(dot) = value.find('.') else {
        return false;
    };
    if dot == 1 {
        return false;
    }
    if !bytes[1..dot]
        .iter()
        .all(|byte| byte.is_ascii_hexdigit())
    {
        return false;
    }
    is_valid_component(&value[dot + 1..], |byte| {
        is_unreserved(byte) || is_sub_delim(byte) || byte == b':'
    })
}

/// Validates a path for the authority/scheme shape it appears in.
fn validate_path(path: &str, has_authority: bool, has_scheme: bool) -> Result<(), String> {
    if has_authority && !(path.is_empty() || path.starts_with('/')) {
        return Err(malformed());
    }
    if !has_authority && !has_scheme && path.starts_with("//") {
        return Err(malformed());
    }
    if is_valid_component(path, |byte| {
        is_unreserved(byte) || is_sub_delim(byte) || matches!(byte, b':' | b'@' | b'/')
    }) {
        Ok(())
    } else {
        Err(malformed())
    }
}

/// Validates a query or fragment component.
pub(crate) fn is_valid_query_or_fragment(value: &str) -> bool {
    is_valid_component(value, |byte| {
        is_unreserved(byte) || is_sub_delim(byte) || matches!(byte, b':' | b'@' | b'/' | b'?')
    })
}

/// Validates a component against `allowed`, accepting complete percent escapes.
fn is_valid_component(value: &str, allowed: impl Fn(u8) -> bool) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
            continue;
        }
        if !byte.is_ascii() || !allowed(byte) {
            return false;
        }
        index += 1;
    }
    true
}

/// Returns whether `byte` is an RFC 3986 unreserved character.
pub(crate) fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

/// Returns whether `byte` is an RFC 3986 sub-delimiter.
pub(crate) fn is_sub_delim(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

/// Parses a decimal port string into an `i64`, rejecting values above `PHP_INT_MAX`.
pub(crate) fn port_to_i64(value: &str) -> Result<i64, ()> {
    let mut result: i128 = 0;
    for byte in value.bytes() {
        if !byte.is_ascii_digit() {
            return Err(());
        }
        result = result * 10 + i128::from(byte - b'0');
        if result > i128::from(i64::MAX) {
            return Err(());
        }
    }
    Ok(result as i64)
}

/// The PHP-visible diagnostic for an RFC 3986 syntax error.
fn malformed() -> String {
    "The specified URI is malformed".to_string()
}

/// Returns whether `value` is a valid path when written without a leading slash check.
pub(crate) fn is_valid_path_for_write(path: &str, has_authority: bool) -> bool {
    if path.is_empty() {
        return true;
    }
    if has_authority && !path.starts_with('/') {
        return false;
    }
    validate_path(path, has_authority, true).is_ok()
}
