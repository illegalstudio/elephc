//! Purpose:
//! WHATWG URL support for `Uri\WhatWg\Url`: parsing relative to a base URL, component
//! reads, mutation, serialization, and validation logging.
//!
//! Called from:
//! - `crate::UriValue` for every WHATWG URL operation exposed through the C ABI.
//!
//! Key details:
//! - Parsing and normalization use the pure-Rust `url` crate, which implements the WHATWG
//!   URL standard; this module adds PHP's empty-credential rules, lexbor's warning list,
//!   and php-src's diagnostic strings and error-type mapping.
//! - A URL that is not `cannot_be_a_base` always has a host slot: `scheme://` reports an
//!   empty host string, while `scheme:` returns PHP null.
//! - Serialization in Unicode mode only replaces the host; path, query, and fragment stay
//!   percent-encoded, matching `Uri\WhatWg\Url::toUnicodeString()`.

use url::{Host, ParseError, Url};

use crate::errors::{
    entry, type_name, UriError, ValidationError, DOMAIN_INVALID_CODE_POINT,
    FILE_INVALID_WINDOWS_DRIVE_LETTER, HOST_INVALID_CODE_POINT, HOST_MISSING, INVALID_CREDENTIALS,
    INVALID_URL_UNIT, IPV4_EMPTY_PART, IPV4_NON_DECIMAL_PART, IPV4_NON_NUMERIC_PART,
    INVALID_REVERSE_SOLIDUS, IPV4_OUT_OF_RANGE_PART, IPV4_TOO_MANY_PARTS,
    IPV6_INVALID_CODE_POINT, IPV6_MULTIPLE_COMPRESSION, IPV6_TOO_MANY_PIECES, IPV6_UNCLOSED,
    MISSING_SCHEME_NON_RELATIVE_URL, PORT_INVALID, PORT_OUT_OF_RANGE,
    SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
};

/// Component selectors shared with the C ABI.
use crate::rfc3986::{
    COMPONENT_FRAGMENT, COMPONENT_HOST, COMPONENT_PASSWORD, COMPONENT_PATH, COMPONENT_QUERY,
    COMPONENT_SCHEME, COMPONENT_USERNAME, MODE_NORMALIZED_UNICODE,
};

/// A parsed WHATWG URL.
#[derive(Clone, Debug)]
pub struct WhatWgUri {
    /// The underlying `url` crate value.
    pub(crate) url: Url,
    /// Host override for labels the `idna` crate refuses as too long.
    ///
    /// `(ascii, unicode)`; the underlying URL carries a short placeholder host while every
    /// host read and serialization uses this pair.
    long_host: Option<(String, String)>,
    /// Path override for an empty-host `file:` URL with more than three leading solidi.
    ///
    /// The `url` crate collapses `file:////x` to `file:///x`; lexbor preserves the extra
    /// solidus in the path, so reads and serialization use this value instead.
    file_path: Option<String>,
}

/// Parses `input` against an optional base URL.
pub fn parse(
    input: &str,
    base: Option<&Url>,
) -> Result<(WhatWgUri, Vec<ValidationError>), UriError> {
    let trimmed = trim_bounds(input);
    let base_is_file = base.is_some_and(|url| url.scheme() == "file");
    let mut log = collect_trim_warnings(input, trimmed);
    if base_is_file {
        append_drive_warnings(input, trimmed, &mut log);
    }

    match parse_url(input, base) {
        Ok(mut url) => {
            normalize_path_encoding(&mut url);
            if url.host_str().is_some_and(|host| {
                !host.starts_with('[') && host_has_forbidden_domain_code_point(host)
            }) {
                // The `url` crate's UTS #46 path admits forbidden domain code points inside a
                // non-ASCII label (for example a space next to `bücher`); the WHATWG host parser
                // rejects them. Re-check the produced ASCII host and report php-src's error.
                let mut failures = failure_log(input, trimmed, base, ParseError::IdnaError);
                log.append(&mut failures);
                let errors = log.into_iter().rev().collect::<Vec<_>>();
                let message = malformed_message("URI", &errors);
                return Err(UriError { message, errors });
            }
            let file_path = file_path_override(&input[trimmed.0..trimmed.1], &url);
            if let Some(fixed) = file_drive_pipe_fix(&input[trimmed.0..trimmed.1], &url) {
                url.set_path(&fixed);
                // The pipe itself stays in the raw input, so `append_component_warnings`
                // reports the InvalidUrlUnit entry at the same position lexbor does.
            }
            append_host_warnings(input, trimmed, &mut log);
            let special = scheme_of(&input[trimmed.0..trimmed.1])
                .map(is_special)
                .unwrap_or(false);
            append_component_warnings(input, trimmed, special, &mut log);
            Ok((
                WhatWgUri { url, long_host: None, file_path },
                log.into_iter().rev().collect::<Vec<_>>(),
            ))
        }
        Err(ParseError::IdnaError) => match parse_long_idna_host(input, base) {
            Some((url, long_host)) => Ok((
                WhatWgUri { url, long_host: Some(long_host), file_path: None },
                log.into_iter().rev().collect::<Vec<_>>(),
            )),
            None => {
                let mut failures = failure_log(
                    input,
                    trimmed,
                    base,
                    ParseError::IdnaError,
                );
                log.append(&mut failures);
                let errors = log.into_iter().rev().collect::<Vec<_>>();
                let message = malformed_message("URI", &errors);
                Err(UriError { message, errors })
            }
        },
        Err(error) => {
            let mut failures = failure_log(input, trimmed, base, error);
            log.append(&mut failures);
            let errors = log.into_iter().rev().collect::<Vec<_>>();
            let message = malformed_message("URI", &errors);
            Err(UriError { message, errors })
        }
    }
}

/// Returns whether `host` contains a WHATWG forbidden domain code point.
///
/// Forbidden domain code points are the forbidden host code points plus C0 controls and the
/// space. A parsed ASCII host can still carry one when a non-ASCII label bypassed the `idna`
/// crate's ASCII deny list, so the WHATWG host parser's final check is repeated here.
fn host_has_forbidden_domain_code_point(host: &str) -> bool {
    host.chars().any(|character| {
        matches!(character, '\u{0000}'..='\u{0020}')
            || matches!(
                character,
                '#' | '/' | ':' | '<' | '>' | '?' | '@' | '[' | '\\' | ']' | '^' | '|'
            )
    })
}

/// Parses with the `url` crate, joining against the base when one is supplied.
fn parse_url(input: &str, base: Option<&Url>) -> Result<Url, ParseError> {
    match base {
        Some(base) => base.join(input),
        None => Url::parse(input),
    }
}

/// Returns the leading and trailing trim boundaries lexbor applies before parsing.
fn trim_bounds(input: &str) -> (usize, usize) {
    let mut start = 0;
    let mut end = input.len();
    for (index, character) in input.char_indices() {
        if character as u32 > 0x20 {
            start = index;
            break;
        }
        start = index + character.len_utf8();
    }
    for (index, character) in input.char_indices().rev() {
        if character as u32 > 0x1F && character != ' ' {
            end = index + character.len_utf8();
            break;
        }
        end = index;
    }
    // All-whitespace input trims to an empty range; every consumer slices `input[start..end]`,
    // so keep the range ordered instead of panicking at the C ABI boundary.
    (start, end.max(start))
}

/// Collects InvalidUrlUnit warnings for leading/trailing trim and tab/newline removal.
fn collect_trim_warnings(input: &str, (start, end): (usize, usize)) -> Vec<ValidationError> {
    let mut log = Vec::new();
    if start > 0 {
        log.push(entry(input.as_bytes(), INVALID_URL_UNIT, false));
    }
    if end < input.len() {
        log.push(entry(&input.as_bytes()[end..], INVALID_URL_UNIT, false));
    }
    if let Some(offset) = input[start..end]
        .find(['\t', '\n', '\r'])
        .map(|offset| start + offset)
    {
        log.push(entry(&input.as_bytes()[offset..], INVALID_URL_UNIT, false));
    }
    log
}

/// Adds lexbor's file-URL drive-letter warnings for a `c|/x` style reference.
fn append_drive_warnings(
    input: &str,
    (start, end): (usize, usize),
    log: &mut Vec<ValidationError>,
) {
    let trimmed = &input[start..end];
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b'|' {
        log.push(entry(
            &input.as_bytes()[start..end],
            FILE_INVALID_WINDOWS_DRIVE_LETTER,
            false,
        ));
        log.push(entry(&input.as_bytes()[start + 1..], INVALID_URL_UNIT, false));
    }
}

/// Adds InvalidUrlUnit warnings for URL code points lexbor rejects in path/query/fragment.
fn append_component_warnings(
    input: &str,
    (start, end): (usize, usize),
    special: bool,
    log: &mut Vec<ValidationError>,
) {
    let trimmed = &input[start..end];
    let Some(component_start) = component_tail_start(trimmed) else {
        return;
    };
    let bytes = trimmed.as_bytes();
    let mut index = component_start;
    while index < bytes.len() {
        let byte = bytes[index];
        let invalid = if byte == b'%' {
            index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
        } else if byte == b'\\' && special {
            // A special URL converts `\\` to `/` while parsing; `append_host_warnings` reports
            // that as InvalidReverseSoldius instead of InvalidUrlUnit.
            false
        } else if byte <= 0x20 {
            // Tab, newline, and carriage return are removed by lexbor's preprocessing pass;
            // `collect_trim_warnings` reports the first one exactly once. Only a space is a
            // component-level warning here.
            byte == b' '
        } else {
            byte == 0x7f
                || matches!(
                    byte,
                    b'"' | b'<' | b'>' | b'`' | b'[' | b']' | b'^' | b'{' | b'|' | b'}' | b'\\'
                )
        };
        if invalid {
            log.push(entry(&input.as_bytes()[start + index..], INVALID_URL_UNIT, false));
        }
        index += 1;
    }
}

/// Returns the byte index where the path (or opaque path) starts.
fn component_tail_start(input: &str) -> Option<usize> {
    let scheme_end = input.find(':')? + 1;
    let rest = &input[scheme_end..];
    if rest.starts_with("//") {
        let authority = &rest[2..];
        let end = authority
            .find(['/', '?', '#'])
            .map(|offset| scheme_end + 2 + offset)
            .unwrap_or(input.len());
        Some(end)
    } else {
        Some(scheme_end)
    }
}

/// Adds the successful-parse warnings lexbor reports for special schemes and credentials.
///
/// The whole log is reversed once at the end of `parse`, so entries are appended here in the
/// opposite order of php-src's observable array: special-scheme warnings first, credentials
/// second, IPv4 non-decimal warnings third, and component warnings last.
fn append_host_warnings(
    input: &str,
    (start, end): (usize, usize),
    log: &mut Vec<ValidationError>,
) {
    let trimmed = &input[start..end];
    let Some((scheme, scheme_end)) = scheme_of(trimmed).map(|scheme| (scheme, trimmed.find(':').unwrap_or(0))) else {
        return;
    };
    let rest = &trimmed[scheme_end + 1..];
    let special = is_special(scheme);
    if special {
        let separators = rest
            .bytes()
            .take_while(|byte| *byte == b'/' || *byte == b'\\')
            .count();
        let is_file = scheme.eq_ignore_ascii_case("file");
        if is_file {
            // `file://` already has its authority; `file:/x`, `file:\\x`, and `file:x`
            // report the missing solidus once. Long mixed separator runs stay permissive.
            if !rest.starts_with("//") && !rest.is_empty() {
                log.push(entry(
                    &input.as_bytes()[start + scheme_end + 1..],
                    SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                    false,
                ));
            }
        } else if rest.starts_with("//") {
            // The authority is already present; each later backslash in the separator run is
            // reported once (for example `https://\x`).
            for offset in 2..separators {
                if rest.as_bytes()[offset] == b'\\' {
                    log.push(entry(
                        &input.as_bytes()[start + scheme_end + 1 + offset..],
                        SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                        false,
                    ));
                }
            }
        } else if separators > 0 {
            // A single solidus/backslash or a run of them: lexbor reports the whole run twice
            // and every later suffix once.
            for _ in 0..2 {
                log.push(entry(
                    &input.as_bytes()[start + scheme_end + 1..],
                    SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                    false,
                ));
            }
            for offset in 1..separators {
                log.push(entry(
                    &input.as_bytes()[start + scheme_end + 1 + offset..],
                    SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                    false,
                ));
            }
        } else if !rest.is_empty() {
            // `https:x`: one warning at the missing separator position.
            log.push(entry(
                &input.as_bytes()[start + scheme_end + 1..],
                SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                false,
            ));
        }
        // A special URL treats every later `\\` as a solidus; lexbor warns once per occurrence.
        if is_file {
            if rest.starts_with('\\') {
                log.push(entry(
                    &input.as_bytes()[start + scheme_end + 1..],
                    INVALID_REVERSE_SOLIDUS,
                    false,
                ));
            }
            for offset in 1..separators {
                if rest.as_bytes()[offset] == b'\\' {
                    log.push(entry(
                        &input.as_bytes()[start + scheme_end + 1 + offset..],
                        INVALID_REVERSE_SOLIDUS,
                        false,
                    ));
                }
            }
        }
        for (offset, character) in rest[separators..].char_indices() {
            if character == '\\' {
                log.push(entry(
                    &input.as_bytes()[start + scheme_end + 1 + separators + offset..],
                    INVALID_REVERSE_SOLIDUS,
                    false,
                ));
            }
        }
    }
    if let Some((authority_start, authority_end)) = authority_bounds(trimmed) {
        let authority = &trimmed[authority_start..authority_end];
        if let Some(at) = authority.rfind('@') {
            log.push(entry(
                &input.as_bytes()[start + authority_start + at..],
                INVALID_CREDENTIALS,
                false,
            ));
        }
        append_ipv4_warnings(input, start, authority_start, authority_end, trimmed, log);
    }
}

/// Warns for non-decimal IPv4 parts, matching lexbor's diagnostics.
///
/// Decimal parts with a leading zero report the part-to-host suffix. A hexadecimal part
/// reports the path suffix for the first part and the part text for later parts; a value above
/// 255 additionally reports `Ipv4OutOfRangePart`.
fn append_ipv4_warnings(
    input: &str,
    start: usize,
    authority_start: usize,
    authority_end: usize,
    trimmed: &str,
    log: &mut Vec<ValidationError>,
) {
    let authority = &trimmed[authority_start..authority_end];
    let after_userinfo = authority
        .rsplit_once('@')
        .map_or(authority, |(_, rest)| rest);
    let host = after_userinfo.split(':').next().unwrap_or(after_userinfo);
    if host.starts_with('[') || host.is_empty() {
        return;
    }
    let parts = host.split('.').collect::<Vec<_>>();
    if parts.len() > 4 {
        return;
    }
    let host_offset = authority.len() - after_userinfo.len();
    let path_context = path_context_bytes(input, start, authority_end, trimmed);
    let mut hex_parts: Vec<(usize, &str, u32)> = Vec::new();
    let mut decimal_leading_zero: Vec<(usize, &str)> = Vec::new();
    let mut offset = host_offset;
    let mut all_ipv4 = true;
    for part in &parts {
        if part.is_empty() {
            all_ipv4 = false;
            break;
        }
        if let Some(hex) = part
            .strip_prefix("0x")
            .or_else(|| part.strip_prefix("0X"))
        {
            if hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                all_ipv4 = false;
                break;
            }
            hex_parts.push((offset, part, u32::from_str_radix(hex, 16).unwrap_or(u32::MAX)));
        } else if part.bytes().all(|byte| byte.is_ascii_digit()) {
            if part.len() > 1 && part.starts_with('0') {
                decimal_leading_zero.push((offset, part));
            }
        } else {
            all_ipv4 = false;
            break;
        }
        offset += part.len() + 1;
    }
    if !all_ipv4 {
        return;
    }
    if !hex_parts.is_empty() {
        // Accumulate in the reverse of php-src's array order; `parse` reverses the whole log.
        // The first hex part uses the path when it is the only part or a later part is also
        // out of range; otherwise lexbor reports the host suffix. Later parts name themselves.
        let later_out_of_range = hex_parts.iter().skip(1).any(|(_, _, value)| *value > 255);
        let first_context = if hex_parts.len() > 1 && !later_out_of_range {
            &input.as_bytes()[start + authority_start + hex_parts[0].0..start + authority_end]
        } else {
            path_context
        };
        for (index, (part_offset, part, _)) in hex_parts.iter().enumerate() {
            let context = if index == 0 {
                first_context
            } else {
                &input.as_bytes()[start + authority_start + part_offset
                    ..start + authority_start + part_offset + part.len()]
            };
            log.push(entry(context, IPV4_NON_DECIMAL_PART, false));
        }
        for (index, (part_offset, part, value)) in hex_parts.iter().enumerate() {
            if *value > 255 {
                let context = if index == 0 {
                    path_context
                } else {
                    &input.as_bytes()[start + authority_start + part_offset
                        ..start + authority_start + part_offset + part.len()]
                };
                log.push(entry(context, IPV4_OUT_OF_RANGE_PART, true));
            }
        }
        return;
    }
    for (part_offset, _) in decimal_leading_zero {
        log.push(entry(
            &input.as_bytes()[start + authority_start + part_offset..start + authority_end],
            IPV4_NON_DECIMAL_PART,
            false,
        ));
    }
}

/// Returns the serialized path bytes for an IPv4 host, or `b"/"` when the input omits it.
fn path_context_bytes<'a>(
    input: &'a str,
    start: usize,
    authority_end: usize,
    trimmed: &str,
) -> &'a [u8] {
    let bytes = trimmed.as_bytes();
    if bytes.get(authority_end) != Some(&b'/') {
        return b"/";
    }
    let path_end = trimmed[authority_end..]
        .find(['?', '#'])
        .map(|offset| authority_end + offset)
        .unwrap_or(trimmed.len());
    &input.as_bytes()[start + authority_end..start + path_end]
}

/// Returns the `[scheme, authority)` bounds of a parsed URL, relative to the trimmed input.
fn authority_bounds(trimmed: &str) -> Option<(usize, usize)> {
    let scheme_end = trimmed.find(':')?;
    let scheme = &trimmed[..scheme_end];
    let rest = &trimmed[scheme_end + 1..];
    let authority_start = if rest.starts_with("//") {
        scheme_end + 3
    } else if is_special(scheme) && rest.starts_with('/') {
        scheme_end + 2
    } else {
        return None;
    };
    let authority_end = trimmed[authority_start..]
        .find(['/', '?', '#'])
        .map(|offset| authority_start + offset)
        .unwrap_or(trimmed.len());
    Some((authority_start, authority_end))
}

/// Classifies a rejected bracketed host into php-src's IPv6 failure kinds.
fn ipv6_error_code(input: &str) -> i64 {
    let Some(open) = input.find('[') else {
        return IPV6_INVALID_CODE_POINT;
    };
    let rest = &input[open + 1..];
    let (inner, closed) = match rest.find(']') {
        Some(close) => (&rest[..close], true),
        None => (
            rest.split(['/', '?', '#']).next().unwrap_or(rest),
            false,
        ),
    };
    if !closed {
        return IPV6_UNCLOSED;
    }
    if inner.matches("::").count() > 1 {
        return IPV6_MULTIPLE_COMPRESSION;
    }
    let compressed = inner.contains("::");
    let pieces = inner
        .split(':')
        .filter(|piece| !piece.is_empty())
        .count();
    if pieces > if compressed { 7 } else { 8 } {
        return IPV6_TOO_MANY_PIECES;
    }
    IPV6_INVALID_CODE_POINT
}

/// Builds the failure log (in lexbor log order) for one `url` parser error.
fn failure_log(
    input: &str,
    (start, end): (usize, usize),
    base: Option<&Url>,
    error: ParseError,
) -> Vec<ValidationError> {
    let trimmed = &input[start..end];
    let context = input.as_bytes();
    match error {
        ParseError::RelativeUrlWithoutBase => vec![entry(
            &context[start..end],
            MISSING_SCHEME_NON_RELATIVE_URL,
            true,
        )],
        ParseError::RelativeUrlWithCannotBeABaseBase => {
            vec![entry(&context[start..end], INVALID_URL_UNIT, true)]
        }
        ParseError::EmptyHost => vec![entry(&context[start..end], HOST_MISSING, true)],
        ParseError::InvalidPort => port_failure_log(context, start, end, trimmed),
        ParseError::InvalidIpv4Address => {
            vec![entry(
                &context[start..end],
                ipv4_error_code(trimmed),
                ipv4_error_code(trimmed) != IPV4_EMPTY_PART,
            )]
        }
        ParseError::InvalidIpv6Address => {
            vec![entry(&context[start..end], ipv6_error_code(trimmed), true)]
        }
        ParseError::InvalidDomainCharacter => {
            let special = base
                .map(|url| is_special(url.scheme()))
                .or_else(|| scheme_of(trimmed).map(is_special))
                .unwrap_or(false);
            vec![entry(
                &context[start..end],
                if special {
                    DOMAIN_INVALID_CODE_POINT
                } else {
                    HOST_INVALID_CODE_POINT
                },
                true,
            )]
        }
        ParseError::IdnaError => {
            vec![entry(&context[start..end], DOMAIN_INVALID_CODE_POINT, true)]
        }
        ParseError::SetHostOnCannotBeABaseUrl => {
            vec![entry(&context[start..end], HOST_MISSING, true)]
        }
        ParseError::Overflow => vec![entry(&context[start..end], INVALID_URL_UNIT, true)],
        _ => vec![entry(&context[start..end], INVALID_URL_UNIT, true)],
    }
}

/// Builds the two-entry log php-src produces for `host:port@user:password` input.
fn port_failure_log(
    context: &[u8],
    start: usize,
    end: usize,
    trimmed: &str,
) -> Vec<ValidationError> {
    if let Some(at) = trimmed.find('@') {
        let absolute_at = start + at;
        let after = &trimmed[at + 1..];
        let port_offset = after
            .rfind(':')
            .map(|offset| absolute_at + 1 + offset + 1)
            .unwrap_or(absolute_at + 1);
        return vec![
            entry(&context[absolute_at..end], crate::errors::INVALID_CREDENTIALS, false),
            entry(&context[port_offset..end], PORT_INVALID, true),
        ];
    }
    let port_offset = trimmed
        .rfind(':')
        .map(|offset| start + offset + 1)
        .unwrap_or(start);
    let port_text = &trimmed[port_offset - start..];
    let port_text = port_text
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(port_text);
    let all_digits = !port_text.is_empty() && port_text.bytes().all(|byte| byte.is_ascii_digit());
    let code = if all_digits
        && port_text
            .parse::<u64>()
            .map_or(true, |value| value > u64::from(u16::MAX))
    {
        PORT_OUT_OF_RANGE
    } else {
        PORT_INVALID
    };
    vec![entry(&context[port_offset..end], code, true)]
}

/// Returns the first IPv4 failure kind visible in a host that the `url` crate rejected.
fn ipv4_error_code(input: &str) -> i64 {
    let host = host_text(input).unwrap_or(input);
    let host = host.split(':').next().unwrap_or(host);
    let parts = host.split('.').collect::<Vec<_>>();
    if parts.len() > 4 {
        IPV4_TOO_MANY_PARTS
    } else if parts.iter().any(|part| part.is_empty()) {
        IPV4_EMPTY_PART
    } else if parts
        .iter()
        .any(|part| !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        IPV4_NON_NUMERIC_PART
    } else if parts
        .iter()
        .any(|part| part.parse::<u32>().map_or(true, |value| value > 255))
    {
        IPV4_OUT_OF_RANGE_PART
    } else {
        IPV4_NON_NUMERIC_PART
    }
}

/// Extracts the authority host text (without userinfo or port) from a raw URL.
fn host_text(input: &str) -> Option<&str> {
    let scheme_end = input.find(':')?;
    let rest = input.get(scheme_end + 1..)?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    if host_port.starts_with('[') {
        return host_port.split(']').next().map(|text| &text[1..]);
    }
    Some(host_port.split(':').next().unwrap_or(host_port))
}

/// Returns the scheme of a raw URL, lowercased, when one is present.
fn scheme_of(input: &str) -> Option<&str> {
    let end = input.find(':')?;
    input.get(..end)
}

/// Builds the PHP message for a component failure.
fn malformed_message(component: &str, errors: &[ValidationError]) -> String {
    let reason = errors
        .iter()
        .rev()
        .find(|error| error.failure)
        .map(|error| type_name(error.type_code));
    match reason {
        Some(reason) => format!("The specified {component} is malformed ({reason})"),
        None => format!("The specified {component} is malformed"),
    }
}

/// Returns whether a scheme is one of WHATWG's special schemes.
pub(crate) fn is_special(scheme: &str) -> bool {
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "ftp" | "file" | "http" | "https" | "ws" | "wss"
    )
}

impl WhatWgUri {
    /// Returns one component in the requested read mode.
    pub fn component(&self, component: i64, mode: i64) -> Option<String> {
        match component {
            COMPONENT_SCHEME => Some(self.url.scheme().to_string()),
            COMPONENT_USERNAME => {
                let password = self.url.password().unwrap_or("");
                if self.url.username().is_empty() && password.is_empty() {
                    None
                } else {
                    Some(self.url.username().to_string())
                }
            }
            COMPONENT_PASSWORD => {
                let password = self.url.password().unwrap_or("");
                if self.url.username().is_empty() && password.is_empty() {
                    None
                } else {
                    Some(password.to_string())
                }
            }
            COMPONENT_HOST => {
                if let Some((ascii, unicode)) = &self.long_host {
                    return Some(if mode == MODE_NORMALIZED_UNICODE {
                        unicode.clone()
                    } else {
                        ascii.clone()
                    });
                }
                if !self.url.has_authority() {
                    return None;
                }
                if mode == MODE_NORMALIZED_UNICODE {
                    self.unicode_host()
                } else {
                    Some(self.url.host_str().unwrap_or("").to_string())
                }
            }
            COMPONENT_PATH => Some(
                if self.url.scheme() == "file" {
                    self.file_path
                        .clone()
                        .unwrap_or_else(|| self.url.path().to_string())
                } else {
                    self.url.path().to_string()
                },
            ),
            COMPONENT_QUERY => self.url.query().map(str::to_string),
            COMPONENT_FRAGMENT => self.url.fragment().map(str::to_string),
            _ => None,
        }
    }

    /// Returns the number of path segments PHP's parser retains for this URL.
    pub fn charge_units(&self) -> i64 {
        let path = self.url.path();
        let segments = path.split('/').count() as i64;
        let non_empty = path.split('/').filter(|part| !part.is_empty()).count() as i64;
        4 * segments + 2 * non_empty
    }

    /// Returns the port, or `None` when it is absent or the scheme default.
    pub fn port_value(&self) -> Option<i64> {
        self.url.port().map(i64::from)
    }

    /// Returns the Unicode form of the host, if the URL has a host slot.
    fn unicode_host(&self) -> Option<String> {
        match self.url.host() {
            Some(Host::Domain(domain)) => Some(idna::domain_to_unicode(domain).0),
            Some(Host::Ipv4(address)) => Some(address.to_string()),
            Some(Host::Ipv6(address)) => Some(format!("[{address}]")),
            None => Some(String::new()),
        }
    }

    /// Serializes the URL in ASCII or Unicode mode, optionally excluding the fragment.
    pub fn to_string(&self, unicode: bool, exclude_fragment: bool) -> String {
        let mut text = if unicode {
            self.unicode_string()
        } else {
            self.url.as_str().to_string()
        };
        if let Some((ascii, unicode_host)) = &self.long_host {
            let replacement = if unicode { unicode_host } else { ascii };
            text = replace_serialized_host(&text, &self.url, replacement);
        }
        if let Some(path) = &self.file_path {
            text = replace_empty_host_file_path(&text, path);
        }
        strip_fragment(text, exclude_fragment)
    }

    /// Replaces the ASCII host with its Unicode form in the serialized URL.
    fn unicode_string(&self) -> String {
        let serialized = self.url.as_str();
        let Some(ascii_host) = self.url.host_str() else {
            return serialized.to_string();
        };
        let Some(unicode_host) = self.unicode_host() else {
            return serialized.to_string();
        };
        if ascii_host.is_empty() || ascii_host == unicode_host {
            return serialized.to_string();
        }
        let Some(scheme_end) = serialized.find(':') else {
            return serialized.to_string();
        };
        let mut start = scheme_end + 1;
        if !serialized[start..].starts_with("//") {
            return serialized.to_string();
        }
        start += 2;
        let authority_end = serialized[start..]
            .find(['/', '?', '#'])
            .map(|offset| start + offset)
            .unwrap_or(serialized.len());
        if let Some(at) = serialized[start..authority_end].rfind('@') {
            start += at + 1;
        }
        let end = start + ascii_host.len();
        if end > serialized.len() {
            return serialized.to_string();
        }
        let mut out = String::with_capacity(serialized.len() + unicode_host.len());
        out.push_str(&serialized[..start]);
        out.push_str(&unicode_host);
        out.push_str(&serialized[end..]);
        out
    }

    /// Returns whether two URLs are equivalent under normalized comparison.
    pub fn equals(&self, other: &WhatWgUri, include_fragment: bool) -> bool {
        self.to_string(false, !include_fragment) == other.to_string(false, !include_fragment)
    }

    /// Writes one string-or-null component into a clone of this URL.
    pub fn with_component(&self, component: i64, value: Option<&str>) -> Result<WhatWgUri, UriError> {
        let mut next = self.clone();
        match component {
            COMPONENT_SCHEME => {
                write_scheme(&mut next.url, value.unwrap_or(""))?;
                next.file_path = None;
            }
            COMPONENT_USERNAME => write_username(&mut next.url, value.unwrap_or("")),
            COMPONENT_PASSWORD => write_password(&mut next.url, value.unwrap_or("")),
            COMPONENT_HOST => {
                write_host(&mut next.url, value)?;
                next.long_host = None;
                next.file_path = None;
            }
            COMPONENT_PATH => {
                next.url.set_path(value.unwrap_or(""));
                normalize_path_encoding(&mut next.url);
                next.file_path = None;
            }
            COMPONENT_QUERY => {
                let value = match value {
                    Some("") | None => None,
                    Some(value) => Some(strip_leading(value)),
                };
                next.url.set_query(value);
            }
            COMPONENT_FRAGMENT => {
                let value = match value {
                    Some("") | None => None,
                    Some(value) => Some(strip_leading_hash(value)),
                };
                next.url.set_fragment(value);
            }
            _ => {
                return Err(UriError {
                    message: "Failed to update the URL".to_string(),
                    errors: Vec::new(),
                })
            }
        }
        Ok(next)
    }

    /// Writes the port into a clone of this URL.
    pub fn with_port(&self, port: Option<i64>) -> Result<WhatWgUri, UriError> {
        let mut next = self.clone();
        match port {
            None => {
                if can_set_authority_component(&next.url) {
                    let _ = next.url.set_port(None);
                }
            }
            Some(port) => {
                if port < 0 {
                    return Err(UriError {
                        message: "The specified port is malformed".to_string(),
                        errors: Vec::new(),
                    });
                }
                if port > i64::from(u16::MAX) {
                    return Err(component_error(
                        "port",
                        PORT_OUT_OF_RANGE,
                        b"",
                    ));
                }
                if can_set_authority_component(&next.url) {
                    let _ = next.url.set_port(Some(port as u16));
                }
            }
        }
        Ok(next)
    }
}

/// Applies lexbor's extra path encodings: `^` everywhere, and only the last trailing
/// space of an opaque path (the URL crate strips opaque trailing spaces).
fn normalize_path_encoding(url: &mut Url) {
    let mut path = url.path().to_string();
    if url.cannot_be_a_base() {
        // An opaque path keeps interior spaces literal; only a trailing space (the one before
        // a query or fragment) is percent-encoded. The `url` crate has already stripped a
        // genuine end-of-input trailing space, so `ends_with(' ')` identifies the lexbor case.
        if path.ends_with(' ') {
            path.pop();
            path.push_str("%20");
        }
    } else if path.contains('^') {
        // `^` is not a URL code point, so the special-scheme path parser encodes it. Opaque
        // paths keep it literal and only warn.
        path = path.replace('^', "%5E");
    }
    if path != url.path() {
        url.set_path(&path);
    }
}

/// Removes a leading `?` from a query setter argument.
fn strip_leading(value: &str) -> &str {
    value.strip_prefix('?').unwrap_or(value)
}

/// Removes a leading `#` from a fragment setter argument.
fn strip_leading_hash(value: &str) -> &str {
    value.strip_prefix('#').unwrap_or(value)
}

/// Parses a URL whose host has a label longer than the `idna` crate accepts.
///
/// PHP's lexbor parses such labels (the WHATWG standard has no 63-character DNS limit), while
/// the Rust `idna` crate rejects labels over 1000 code points. This fallback punycodes the long
/// label, parses the rest against a placeholder host, and keeps the real host for reads and
/// serialization.
fn parse_long_idna_host(
    input: &str,
    base: Option<&Url>,
) -> Option<(Url, (String, String))> {
    if base.is_some() {
        return None;
    }
    let (host_start, host_end) = raw_host_bounds(input)?;
    let host = &input[host_start..host_end];
    if host.starts_with('[') {
        return None;
    }
    let host_only = host.rsplit_once(':').map_or(host, |(host, _)| host);
    let mut labels = Vec::new();
    let mut changed = false;
    for label in host_only.split('.') {
        if label.is_ascii() {
            labels.push(label.to_string());
            continue;
        }
        let encoded = match idna::domain_to_ascii(label) {
            Ok(encoded) => encoded,
            // The `idna` crate deliberately caps labels at 1000 code points; beyond that,
            // punycode-encode directly. A shorter invalid label stays invalid.
            Err(_) if label.chars().count() > 1000 => {
                punycode::encode(label).map(|encoded| format!("xn--{encoded}")).ok()?
            }
            Err(_) => return None,
        };
        changed = true;
        labels.push(encoded);
    }
    if !changed {
        return None;
    }
    let ascii_host = labels.join(".");
    if host_has_forbidden_domain_code_point(&ascii_host) {
        return None;
    }
    let unicode_host = host_only.to_ascii_lowercase();
    let mut replaced = String::with_capacity(input.len() + 14);
    replaced.push_str(&input[..host_start]);
    replaced.push_str("elephc.invalid");
    replaced.push_str(&input[host_start + host_only.len()..]);
    let url = Url::parse(&replaced).ok()?;
    Some((url, (ascii_host, unicode_host)))
}

/// Returns the `[start, end)` byte range of the authority host in a raw URL.
fn raw_host_bounds(input: &str) -> Option<(usize, usize)> {
    let scheme_end = input.find("://")? + 3;
    let authority_end = input[scheme_end..]
        .find(['/', '?', '#'])
        .map(|offset| scheme_end + offset)
        .unwrap_or(input.len());
    let authority = &input[scheme_end..authority_end];
    let host_offset = authority.rfind('@').map_or(0, |at| at + 1);
    let host_start = scheme_end + host_offset;
    let host = &input[host_start..authority_end];
    let host_only_len = if host.starts_with('[') {
        host.find(']')? + 1
    } else {
        host.rsplit_once(':').map_or(host.len(), |(host, _)| host.len())
    };
    Some((host_start, host_start + host_only_len))
}

/// Returns the lexbor path for an empty-host `file:` URL with more than three solidi.
fn file_path_override(trimmed: &str, url: &Url) -> Option<String> {
    if url.scheme() != "file" || url.host_str().is_some_and(|host| !host.is_empty()) {
        return None;
    }
    let scheme_end = trimmed.find(':')?;
    let rest = &trimmed[scheme_end + 1..];
    let solidi = rest
        .bytes()
        .take_while(|byte| *byte == b'/' || *byte == b'\\')
        .count();
    if solidi <= 3 {
        return None;
    }
    Some(format!("{}{}", "/".repeat(solidi - 3), url.path()))
}

/// Rewrites `file:///c|/x` to `file:///c:/x`.
fn file_drive_pipe_fix(_trimmed: &str, url: &Url) -> Option<String> {
    if url.scheme() != "file" || url.host_str().is_some_and(|host| !host.is_empty()) {
        return None;
    }
    let path = url.path();
    let bytes = path.as_bytes();
    if bytes.len() < 3
        || bytes[0] != b'/'
        || !bytes[1].is_ascii_alphabetic()
        || bytes[2] != b'|'
        || (bytes.len() > 3 && bytes[3] != b'/')
    {
        return None;
    }
    Some(format!("/{}:{}", bytes[1] as char, &path[3..]))
}

/// Replaces the path of an empty-host file URL while preserving its suffix.
fn replace_empty_host_file_path(serialized: &str, path: &str) -> String {
    let prefix = "file://";
    let Some(tail) = serialized.strip_prefix(prefix) else {
        return serialized.to_string();
    };
    if !tail.starts_with('/') {
        return serialized.to_string();
    }
    let end = tail.find(['?', '#']).unwrap_or(tail.len());
    let mut out = String::with_capacity(serialized.len() + path.len());
    out.push_str(prefix);
    out.push_str(path);
    out.push_str(&tail[end..]);
    out
}

/// Replaces the serialized host with an override while preserving userinfo, port, and suffix.
fn replace_serialized_host(serialized: &str, url: &Url, replacement: &str) -> String {
    let Some(host) = url.host_str() else {
        return serialized.to_string();
    };
    if host.is_empty() {
        return serialized.to_string();
    }
    let Some(scheme_end) = serialized.find(':') else {
        return serialized.to_string();
    };
    let mut start = scheme_end + 1;
    if !serialized[start..].starts_with("//") {
        return serialized.to_string();
    }
    start += 2;
    let authority_end = serialized[start..]
        .find(['/', '?', '#'])
        .map(|offset| start + offset)
        .unwrap_or(serialized.len());
    if let Some(at) = serialized[start..authority_end].rfind('@') {
        start += at + 1;
    }
    let end = start + host.len();
    if end > serialized.len() {
        return serialized.to_string();
    }
    let mut out = String::with_capacity(serialized.len() + replacement.len());
    out.push_str(&serialized[..start]);
    out.push_str(replacement);
    out.push_str(&serialized[end..]);
    out
}

/// Removes the fragment from a serialized URL when requested.
fn strip_fragment(mut text: String, exclude: bool) -> String {
    if exclude {
        if let Some(hash) = text.find('#') {
            text.truncate(hash);
        }
    }
    text
}

/// Builds a component failure with one validation entry.
fn component_error(component: &str, code: i64, context: &[u8]) -> UriError {
    let errors = vec![entry(context, code, true)];
    UriError {
        message: malformed_message(component, &errors),
        errors,
    }
}

/// Writes the scheme with WHATWG compatibility and lexbor's delimiter tolerance.
fn write_scheme(url: &mut Url, value: &str) -> Result<(), UriError> {
    let value = value.trim_matches(|character: char| character <= ' ');
    let candidate = value.split(':').next().unwrap_or("");
    if !is_valid_scheme(candidate) {
        return Err(UriError {
            message: "The specified scheme is malformed".to_string(),
            errors: Vec::new(),
        });
    }
    let new_scheme = candidate.to_ascii_lowercase();
    let old_scheme = url.scheme().to_string();
    if new_scheme == old_scheme || !scheme_change_allowed(url, &new_scheme) {
        return Ok(());
    }
    let serialized = url.as_str();
    let rest = &serialized[old_scheme.len()..];
    let rebuilt = format!("{new_scheme}{rest}");
    if let Ok(mut parsed) = Url::parse(&rebuilt) {
        normalize_path_encoding(&mut parsed);
        *url = parsed;
    }
    Ok(())
}

/// Returns whether WHATWG's scheme setter may change the scheme.
fn scheme_change_allowed(url: &Url, new_scheme: &str) -> bool {
    let old_scheme = url.scheme();
    if is_special(old_scheme) != is_special(new_scheme) {
        return false;
    }
    if new_scheme == "file" {
        if !url.username().is_empty()
            || url.password().is_some_and(|password| !password.is_empty())
            || url.port().is_some()
        {
            return false;
        }
    }
    if old_scheme == "file" && url.host_str().is_none_or(str::is_empty) {
        return false;
    }
    true
}

/// Returns whether `value` matches WHATWG's scheme syntax.
fn is_valid_scheme(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && bytes.all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')
        })
}

/// Writes the username when the URL can carry credentials.
fn write_username(url: &mut Url, value: &str) {
    if can_set_authority_component(url) {
        let _ = url.set_username(value);
    }
}

/// Writes the password when the URL can carry credentials.
fn write_password(url: &mut Url, value: &str) {
    if can_set_authority_component(url) {
        let _ = url.set_password(Some(value));
    }
}

/// Returns whether a URL can carry a port or credentials.
fn can_set_authority_component(url: &Url) -> bool {
    url.scheme() != "file" && url.has_host() && url.host_str().is_some_and(|host| !host.is_empty())
}

/// Writes a host value with php-src's message and validation-error mapping.
fn write_host(url: &mut Url, value: Option<&str>) -> Result<(), UriError> {
    if url.cannot_be_a_base() {
        return Ok(());
    }
    let special = is_special(url.scheme());
    let is_file = url.scheme() == "file";
    match value {
        None => {
            if special && !is_file {
                return Err(component_error("host", HOST_MISSING, b""));
            }
            if is_file {
                let _ = url.set_host(None);
            } else {
                let _ = url.set_host(Some(""));
            }
            Ok(())
        }
        Some(value) if value.is_empty() => {
            if special && !is_file {
                return Err(component_error("host", HOST_MISSING, b""));
            }
            if is_file {
                let _ = url.set_host(None);
            } else {
                let _ = url.set_host(Some(""));
            }
            Ok(())
        }
        Some(value) => {
            let value = value.trim_matches(|character: char| character <= ' ');
            let value = value.split('#').next().unwrap_or(value);
            if value.is_empty() {
                if special && !is_file {
                    return Err(component_error("host", HOST_MISSING, b""));
                }
                if is_file {
                    let _ = url.set_host(None);
                } else {
                    let _ = url.set_host(Some(""));
                }
                return Ok(());
            }
            let bracketed = value.starts_with('[') && value.ends_with(']');
            if !bracketed
                && value
                    .bytes()
                    .any(|byte| matches!(byte, b':' | b'/' | b'\\' | b'?' | b'#'))
            {
                return Err(UriError {
                    message: "The specified host is malformed".to_string(),
                    errors: Vec::new(),
                });
            }
            let Some(decoded) = percent_decode(value) else {
                return Err(host_code_error(value, special));
            };
            let decoded = String::from_utf8_lossy(&decoded);
            let forbidden = decoded.contains('@')
                || decoded.contains('\0')
                || (!bracketed && (decoded.contains('[') || decoded.contains(']')))
                || (!bracketed && (decoded.contains(':') || decoded.contains('/')));
            if forbidden {
                return Err(host_code_error(value, special));
            }
            match url.set_host(Some(value)) {
                Ok(()) => Ok(()),
                Err(ParseError::EmptyHost) => Err(component_error("host", HOST_MISSING, value.as_bytes())),
                Err(ParseError::IdnaError)
                | Err(ParseError::InvalidDomainCharacter)
                | Err(ParseError::InvalidIpv4Address)
                | Err(ParseError::InvalidIpv6Address) => Err(host_code_error(value, special)),
                Err(_) => Err(UriError {
                    message: "The specified host is malformed".to_string(),
                    errors: Vec::new(),
                }),
            }
        }
    }
}

/// Builds the host failure php-src reports for a forbidden decoded code point.
fn host_code_error(value: &str, special: bool) -> UriError {
    component_error(
        "host",
        if special {
            DOMAIN_INVALID_CODE_POINT
        } else {
            HOST_INVALID_CODE_POINT
        },
        value.as_bytes(),
    )
}

/// Decodes all valid percent escapes, returning `None` for a malformed escape.
fn percent_decode(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            out.push((high << 4) | low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    Some(out)
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
