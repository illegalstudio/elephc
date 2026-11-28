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

use std::sync::Arc;

use url::{Host, ParseError, Url};

use crate::errors::{
    entry, entry_at, entry_range, type_name, UriError, ValidationError, DOMAIN_INVALID_CODE_POINT,
    FILE_INVALID_WINDOWS_DRIVE_LETTER, FILE_INVALID_WINDOWS_DRIVE_LETTER_HOST,
    HOST_INVALID_CODE_POINT, HOST_MISSING, INVALID_CREDENTIALS, INVALID_REVERSE_SOLIDUS,
    INVALID_URL_UNIT, IPV4_EMPTY_PART, IPV4_IN_IPV6_INVALID_CODE_POINT,
    IPV4_IN_IPV6_OUT_OF_RANGE_PART, IPV4_IN_IPV6_TOO_FEW_PARTS,
    IPV4_IN_IPV6_TOO_MANY_PIECES, IPV4_NON_DECIMAL_PART, IPV4_NON_NUMERIC_PART,
    IPV4_OUT_OF_RANGE_PART, IPV4_TOO_MANY_PARTS, IPV6_INVALID_CODE_POINT,
    IPV6_INVALID_COMPRESSION, IPV6_MULTIPLE_COMPRESSION, IPV6_TOO_FEW_PIECES,
    IPV6_TOO_MANY_PIECES, IPV6_UNCLOSED, MISSING_SCHEME_NON_RELATIVE_URL, PORT_INVALID,
    PORT_OUT_OF_RANGE, SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
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
    /// Whether lexbor ignores subsequent path writes for this parsed URL.
    ///
    /// A relative Windows-drive reference (`c|/x`, `c:/x`) parsed against a `file:` base
    /// yields a URL whose path setter is inert: every `withPath()` returns the same URL.
    /// Absolute file URLs do not have that state.
    path_write_locked: bool,
}

/// Validation log that shares the parsed input instead of copying one suffix per warning.
struct WarningLog {
    /// Complete input bytes, shared by pointer with every input-derived entry.
    source: Arc<[u8]>,
    /// Entries in lexbor log order.
    entries: Vec<ValidationError>,
}

impl WarningLog {
    /// Creates an empty log over one parsed input.
    fn new(source: Arc<[u8]>) -> Self {
        WarningLog {
            source,
            entries: Vec::new(),
        }
    }

    /// Appends an entry whose context is the input suffix starting at `start`.
    fn suffix(&mut self, start: usize, type_code: i64, failure: bool) {
        self.entries
            .push(entry_at(&self.source, start, type_code, failure));
    }

    /// Appends an entry whose context is one input range.
    fn range(&mut self, start: usize, end: usize, type_code: i64, failure: bool) {
        self.entries
            .push(entry_range(&self.source, start, end, type_code, failure));
    }

    /// Appends an entry whose context is not a slice of the parsed input.
    fn owned(&mut self, context: &[u8], type_code: i64, failure: bool) {
        self.entries.push(entry(context, type_code, failure));
    }

    /// Appends entries built from an owned context source.
    fn extend(&mut self, entries: Vec<ValidationError>) {
        self.entries.extend(entries);
    }

    /// Consumes the log and returns its entries.
    fn into_entries(self) -> Vec<ValidationError> {
        self.entries
    }
}

/// Parses `input` against an optional base WHATWG URL, preserving its saved overrides.
pub fn parse(
    input: &str,
    base: Option<&WhatWgUri>,
) -> Result<(WhatWgUri, Vec<ValidationError>), UriError> {
    let trimmed = trim_bounds(input);
    let reference = &input[trimmed.0..trimmed.1];
    let base_is_file = base.is_some_and(|url| url.url.scheme() == "file");
    let source: Arc<[u8]> = Arc::from(input.as_bytes());
    let mut log = WarningLog::new(source);
    collect_trim_warnings(input, trimmed, &mut log);
    if base_is_file {
        append_drive_warnings(input, trimmed, &mut log);
    }

    // Parse the reference lexbor actually sees. A leading C0/space is trimmed before the
    // URL parser runs; handing it the raw input let `Url::join()` trim the space itself and
    // then consume a following backslash as a separator instead of path data.
    match parse_url(reference, base.map(|base| &base.url)) {
        Ok(url) => {
            // This extra domain check repairs the `url` crate's UTS #46 path for SPECIAL
            // schemes. Non-special hosts are opaque and legitimately keep `%XX` escapes or
            // UTF-8 bytes (`foo://exa%20mple.org/`); their host rules are enforced elsewhere.
            if is_special(url.scheme())
                && url.host_str().is_some_and(|host| {
                    !host.starts_with('[') && host_has_forbidden_domain_code_point(host)
                })
            {
                // The `url` crate's UTS #46 path admits forbidden domain code points inside a
                // non-ASCII label (for example a space next to `bücher`); the WHATWG host parser
                // rejects them. Re-check the produced ASCII host and report php-src's error.
                append_special_separator_warnings(
                    input,
                    trimmed,
                    base.map(|base| base.url.scheme()),
                    &mut log,
                );
                domain_failure_log(
                    input,
                    trimmed.0,
                    reference,
                    true,
                    base.map(|base| base.url.scheme()),
                    &mut log,
                );
                let errors = log.into_entries().into_iter().rev().collect::<Vec<_>>();
                let message = malformed_message("URI", &errors);
                return Err(UriError { message, errors });
            }
            let parsed = finish_parsed_url(url, None, input, trimmed, reference, base, &mut log);
            Ok((
                parsed,
                log.into_entries().into_iter().rev().collect::<Vec<_>>(),
            ))
        }
        Err(ParseError::IdnaError) => {
            let relaxed_input = relaxed_reference_input(reference, base);
            match parse_relaxed_idna_host(&relaxed_input) {
                Some((url, long_host)) => {
                    let parsed =
                        finish_parsed_url(url, long_host, input, trimmed, reference, base, &mut log);
                    Ok((
                        parsed,
                        log.into_entries().into_iter().rev().collect::<Vec<_>>(),
                    ))
                }
                None => {
                    // A forbidden domain code point is reported before the ToASCII step. A pure
                    // ToASCII failure logs no domain entry, but the separator diagnostics still
                    // describe the reference text.
                    append_special_separator_warnings(
                        input,
                        trimmed,
                        base.map(|base| base.url.scheme()),
                        &mut log,
                    );
                    if let Some(failure) = forbidden_host_failure(
                        input,
                        trimmed.0,
                        reference,
                        true,
                        base.map(|base| base.url.scheme()),
                    ) {
                        log.entries.push(failure);
                    }
                    let errors = log.into_entries().into_iter().rev().collect::<Vec<_>>();
                    let message = malformed_message("URI", &errors);
                    Err(UriError { message, errors })
                }
            }
        }
        Err(error) => {
            failure_log(input, trimmed, base.map(|base| &base.url), error, &mut log);
            let errors = log.into_entries().into_iter().rev().collect::<Vec<_>>();
            let message = malformed_message("URI", &errors);
            Err(UriError { message, errors })
        }
    }
}

/// Copies the base URL's saved host and file-path overrides onto a resolved reference.
fn inherit_base_overrides(parsed: &mut WhatWgUri, reference: &str, base: Option<&WhatWgUri>) {
    let Some(base) = base else {
        return;
    };
    if !inherits_base_authority(reference) {
        return;
    }
    if let Some(long_host) = &base.long_host {
        // A relative reference keeps the base's placeholder host; every host read and every
        // serialization must keep using the base's real ASCII/Unicode pair.
        if parsed.url.host_str() == base.url.host_str() {
            parsed.long_host = Some(long_host.clone());
        }
    }
    if !reference_path_is_absolute(reference) {
        if let Some(base_path) = &base.file_path {
            let extra = leading_slashes(base_path).saturating_sub(leading_slashes(base.url.path()));
            if extra > 0 {
                let mut path = "/".repeat(extra);
                path.push_str(parsed.url.path());
                parsed.file_path = Some(path);
            }
        }
    }
}

/// Builds the absolute text lexbor's relaxed host parser needs for a network-relative input.
fn relaxed_reference_input(reference: &str, base: Option<&WhatWgUri>) -> String {
    if let Some(rest) = reference.strip_prefix("//") {
        return base.map_or_else(
            || reference.to_string(),
            |base| format!("{}://{rest}", base.url.scheme()),
        );
    }
    if let Some(rest) = reference.strip_prefix("\\\\") {
        return base.map_or_else(
            || reference.to_string(),
            |base| {
                format!(
                    "{}://{}",
                    base.url.scheme(),
                    replace_authority_path_backslashes(rest)
                )
            },
        );
    }
    reference.to_string()
}

/// Replaces backslashes in a network-relative authority/path, leaving query/fragment data.
fn replace_authority_path_backslashes(rest: &str) -> String {
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    let mut out = String::with_capacity(rest.len());
    for (index, character) in rest.char_indices() {
        if index < end && character == '\\' {
            out.push('/');
        } else {
            out.push(character);
        }
    }
    out
}

/// Applies lexbor's finishing steps shared by ordinary and relaxed-host successful parses.
fn finish_parsed_url(
    mut url: Url,
    long_host: Option<(String, String)>,
    input: &str,
    trimmed: (usize, usize),
    reference: &str,
    base: Option<&WhatWgUri>,
    log: &mut WarningLog,
) -> WhatWgUri {
    normalize_path_encoding(&mut url);
    let file_path = file_path_override(reference, &url)
        .or_else(|| relative_file_path_override(reference, &url));
    let path_write_locked =
        url.scheme() == "file" && relative_windows_drive_reference(reference);
    if let Some(fixed) = file_drive_pipe_fix(reference, &url) {
        url.set_path(&fixed);
        // The pipe itself stays in the raw input, so `append_component_warnings`
        // reports the InvalidUrlUnit entry at the same position lexbor does.
    }
    append_host_warnings(input, trimmed, base.map(|base| base.url.scheme()), log);
    let special = scheme_of(reference)
        .map(is_special)
        .or_else(|| base.map(|base| is_special(base.url.scheme())))
        .unwrap_or(false);
    // The host scan owns path reverse solidi for an absolute URL and for a forward-slash
    // network reference. A scheme-less reference without `//` keeps them here; so does the
    // backslash form of a network reference, which the host scan deliberately skips.
    let relative_path_end = if scheme_prefix_end(reference).is_some()
        || (special && reference.starts_with("//"))
    {
        None
    } else {
        Some(relative_component_path_end(reference))
    };
    append_component_warnings(input, trimmed, special, relative_path_end, log);
    let mut parsed = WhatWgUri {
        url,
        long_host,
        file_path,
        path_write_locked,
    };
    inherit_base_overrides(&mut parsed, reference, base);
    parsed
}

/// Returns whether a reference inherits the base URL's authority.
fn inherits_base_authority(reference: &str) -> bool {
    if reference.starts_with("//") || reference.starts_with("\\\\") {
        return false;
    }
    let before_path = reference
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(reference);
    !before_path.contains(':')
}

/// Returns whether a reference's path is absolute (so it drops a file URL's extra solidi).
fn reference_path_is_absolute(reference: &str) -> bool {
    let path = reference.split(['?', '#']).next().unwrap_or(reference);
    path.starts_with('/') || path.starts_with('\\')
}

/// Returns the number of leading solidi in a URL path.
fn leading_slashes(path: &str) -> usize {
    path.bytes().take_while(|byte| *byte == b'/').count()
}

/// Returns whether `host` contains a WHATWG forbidden domain code point.
///
/// Forbidden domain code points are the forbidden host code points plus C0 controls and the
/// space. A parsed ASCII host can still carry one when a non-ASCII label bypassed the `idna`
/// crate's ASCII deny list, so the WHATWG host parser's final check is repeated here.
fn host_has_forbidden_domain_code_point(host: &str) -> bool {
    host.chars().any(|character| {
        matches!(character, '\u{0000}'..='\u{0020}' | '\u{007F}' | '%')
            || matches!(
                character,
                '#' | '/' | ':' | '<' | '>' | '?' | '@' | '[' | '\\' | ']' | '^' | '|'
            )
    })
}

/// Parses with the `url` crate, joining against the base when one is supplied.
fn parse_url(input: &str, base: Option<&Url>) -> Result<Url, ParseError> {
    match base {
        Some(base) => {
            // Non-special schemes treat backslashes as literal path bytes. The `url` crate
            // agrees everywhere except at the START of a relative reference, where it treats
            // them as separators. Prefixing `./` forces a relative path and preserves every
            // backslash (and every percent escape and dot segment) without rewriting bytes.
            // A reference with its own scheme is absolute and keeps that scheme's rules.
            if !is_special(base.scheme())
                && scheme_prefix_end(input).is_none()
                && input.starts_with('\\')
            {
                return base.join(&format!("./{input}"));
            }
            // Separator rules differ by scheme. A special non-file URL treats a run of
            // three or more slashes/backslashes as an authority prefix (`///x` -> `//x`);
            // `file:` and non-special schemes keep the extra separators as PATH bytes.
            // Backslashes are separators only for special schemes.
            let special = is_special(base.scheme());
            let slashes = input
                .bytes()
                .take_while(|byte| *byte == b'/' || (special && *byte == b'\\'))
                .count();
            if slashes < 2 {
                return base.join(input);
            }
            let rest = &input[slashes..];
            if special && base.scheme() != "file" {
                // A network-path reference; lexbor absorbs the extra separators.
                return base.join(&format!("//{rest}"));
            }
            if slashes == 2 {
                if special && input[..slashes].contains('\\') {
                    return base.join(&format!("//{rest}"));
                }
                return base.join(input);
            }
            // Path-only prefix: preserve `slashes - 2` leading separators and empty the
            // authority, which is what lexbor does for file and non-special bases.
            let parsed = base.join(&format!("/{rest}"))?;
            let desired = format!(
                "{}{}",
                "/".repeat(slashes - 2),
                parsed.path().trim_start_matches('/')
            );
            let mut result = base.clone();
            let _ = result.set_username("");
            let _ = result.set_password(None);
            let _ = result.set_host(Some(""));
            let _ = result.set_port(None);
            result.set_path(&desired);
            result.set_query(parsed.query());
            result.set_fragment(parsed.fragment());
            Ok(result)
        }
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
fn collect_trim_warnings(
    input: &str,
    (start, end): (usize, usize),
    log: &mut WarningLog,
) {
    if start > 0 {
        log.suffix(0, INVALID_URL_UNIT, false);
    }
    if end < input.len() {
        log.suffix(end, INVALID_URL_UNIT, false);
    }
    if let Some(offset) = input[start..end]
        .find(['\t', '\n', '\r'])
        .map(|offset| start + offset)
    {
        log.suffix(offset, INVALID_URL_UNIT, false);
    }
}

/// Adds lexbor's special-scheme missing-solidus warnings for the separator run before the
/// authority. Failure paths report these even though the path itself is never parsed.
fn append_special_separator_warnings(
    input: &str,
    (start, end): (usize, usize),
    base_scheme: Option<&str>,
    log: &mut WarningLog,
) {
    let trimmed = &input[start..end];
    let network_relative = scheme_of(trimmed).is_none()
        && (trimmed.starts_with("//") || trimmed.starts_with("\\"));
    let (scheme, rest_start) = if let Some(scheme) = scheme_of(trimmed) {
        (scheme, trimmed.find(':').unwrap_or(0) + 1)
    } else if network_relative {
        let Some(base_scheme) = base_scheme else {
            return;
        };
        (base_scheme, 0)
    } else {
        return;
    };
    let rest = &trimmed[rest_start..];
    let special = is_special(scheme);
    let run_special_block = scheme_of(trimmed).is_some() || rest.starts_with("//");
    if !(special && run_special_block) {
        return;
    }
    let separators = rest
        .bytes()
        .take_while(|byte| *byte == b'/' || *byte == b'\\')
        .count();
    let is_file = scheme.eq_ignore_ascii_case("file");
    if is_file {
        if !rest.starts_with("//") && !rest.is_empty() {
            log.suffix(
                start + rest_start,
                SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                false,
            );
        }
    } else if rest.starts_with("//") {
        for offset in 2..separators {
            log.suffix(
                start + rest_start + offset,
                SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                false,
            );
        }
    } else if separators > 0 {
        for _ in 0..2 {
            log.suffix(
                start + rest_start,
                SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                false,
            );
        }
        for offset in 1..separators {
            log.suffix(
                start + rest_start + offset,
                SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                false,
            );
        }
    } else if !rest.is_empty() {
        log.suffix(
            start + rest_start,
            SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
            false,
        );
    }
}

/// Adds lexbor's file-URL drive-letter warnings for a `c|/x` style reference.
fn append_drive_warnings(
    input: &str,
    (start, end): (usize, usize),
    log: &mut WarningLog,
) {
    let trimmed = &input[start..end];
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b'|' {
        log.range(
            start,
            end,
            FILE_INVALID_WINDOWS_DRIVE_LETTER,
            false,
        );
        // The pipe's InvalidUrlUnit entry is emitted by `append_component_warnings`, which
        // now scans relative references too; emitting it here as well duplicated it.
    }
}

/// Adds InvalidUrlUnit warnings for URL code points lexbor rejects in path/query/fragment.
fn append_component_warnings(
    input: &str,
    (start, end): (usize, usize),
    special: bool,
    relative_path_end: Option<usize>,
    log: &mut WarningLog,
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
            match relative_path_end {
                // Relative special-scheme paths warn once per reverse solidus.
                Some(path_end) if index < path_end => {
                    log.suffix(start + index, INVALID_REVERSE_SOLIDUS, false);
                    false
                }
                // Query and fragment reverse solidi stay InvalidUrlUnit even on a special base.
                Some(_) => true,
                // Absolute and forward-slash network references report path reverse solidi
                // from `append_host_warnings`; their query/fragment backslashes remain
                // URL-unit diagnostics here.
                None => index >= trimmed.find(['?', '#']).unwrap_or(trimmed.len()),
            }
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
            log.suffix(start + index, INVALID_URL_UNIT, false);
        }
        index += 1;
    }
}

/// Returns the byte index just past a syntactically valid `scheme:`, when present.
fn scheme_prefix_end(input: &str) -> Option<usize> {
    let colon = input.find(':')?;
    let scheme = &input[..colon];
    let mut characters = scheme.chars();
    let first = characters.next()?;
    if !first.is_ascii_alphabetic()
        || !characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
    {
        return None;
    }
    Some(colon + 1)
}

/// Returns the byte index where the path (or opaque path) starts.
///
/// A relative reference has no scheme; its path starts at the beginning, or after a
/// network-path authority (`//host/...`). Resolving such references still has to scan the
/// path/query/fragment for lexbor warnings.
fn component_tail_start(input: &str) -> Option<usize> {
    if let Some(scheme_end) = scheme_prefix_end(input) {
        let rest = &input[scheme_end..];
        if rest.starts_with("//") {
            // A special scheme starts its path at a backslash too, so `%` escapes after it
            // belong to the component scan instead of being hidden inside the authority.
            let special = is_special(&input[..scheme_end - 1]);
            let authority = &rest[2..];
            let end = authority
                .find(|character| {
                    matches!(character, '/' | '?' | '#') || (special && character == '\\')
                })
                .map(|offset| scheme_end + 2 + offset)
                .unwrap_or(input.len());
            Some(end)
        } else {
            Some(scheme_end)
        }
    } else if input.starts_with("//") || input.starts_with("\\\\") {
        let authority = &input[2..];
        Some(
            authority
                .find(['/', '\\', '?', '#'])
                .map(|offset| 2 + offset)
                .unwrap_or(input.len()),
        )
    } else {
        Some(0)
    }
}

/// Returns the end of a relative reference's path component (the first query/fragment mark).
fn relative_component_path_end(input: &str) -> usize {
    let start = component_tail_start(input).unwrap_or(0);
    input[start..]
        .find(['?', '#'])
        .map(|offset| start + offset)
        .unwrap_or(input.len())
}

/// Adds the successful-parse warnings lexbor reports for special schemes and credentials.
///
/// The whole log is reversed once at the end of `parse`, so entries are appended here in the
/// opposite order of php-src's observable array: special-scheme warnings first, credentials
/// second, IPv4 non-decimal warnings third, and component warnings last.
fn append_host_warnings(
    input: &str,
    (start, end): (usize, usize),
    base_scheme: Option<&str>,
    log: &mut WarningLog,
) {
    let trimmed = &input[start..end];
    // A network-relative reference has no literal scheme, but resolving it inherits the
    // base's scheme for lexbor's special-scheme and host diagnostics.
    let network_relative = scheme_of(trimmed).is_none()
        && (trimmed.starts_with("//") || trimmed.starts_with("\\"));
    let (scheme, rest_start) = if let Some(scheme) = scheme_of(trimmed) {
        (scheme, trimmed.find(':').unwrap_or(0) + 1)
    } else if network_relative {
        let Some(base_scheme) = base_scheme else {
            return;
        };
        (base_scheme, 0)
    } else {
        return;
    };
    let rest = &trimmed[rest_start..];
    let special = is_special(scheme);
    // The missing-solidus arms describe a written scheme separator. A network-relative
    // forward-slash reference still reports extra separators, while its backslash form is
    // handled by the component scanner's InvalidReverseSoldius entries.
    let run_special_block = scheme_of(trimmed).is_some() || rest.starts_with("//");
    if special && run_special_block {
        let separators = rest
            .bytes()
            .take_while(|byte| *byte == b'/' || *byte == b'\\')
            .count();
        let is_file = scheme.eq_ignore_ascii_case("file");
        if is_file {
            // `file://` already has its authority; `file:/x`, `file:\\x`, and `file:x`
            // report the missing solidus once. Long mixed separator runs stay permissive.
            if !rest.starts_with("//") && !rest.is_empty() {
                log.suffix(
                    start + rest_start,
                    SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                    false,
                );
            }
        } else if rest.starts_with("//") {
            // The authority is already present; every extra slash/backslash in the separator
            // run is reported once (for example `https:///x` and `https://\x`).
            for offset in 2..separators {
                log.suffix(
                    start + rest_start + offset,
                    SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                    false,
                );
            }
        } else if separators > 0 {
            // A single solidus/backslash or a run of them: lexbor reports the whole run twice
            // and every later suffix once.
            for _ in 0..2 {
                log.suffix(
                    start + rest_start,
                    SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                    false,
                );
            }
            for offset in 1..separators {
                log.suffix(
                    start + rest_start + offset,
                    SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                    false,
                );
            }
        } else if !rest.is_empty() {
            // `https:x`: one warning at the missing separator position.
            log.suffix(
                start + rest_start,
                SPECIAL_SCHEME_MISSING_FOLLOWING_SOLIDUS,
                false,
            );
        }
        // A special URL treats every later `\\` as a solidus; lexbor warns once per occurrence.
        if is_file {
            if rest.starts_with('\\') {
                log.suffix(start + rest_start, INVALID_REVERSE_SOLIDUS, false);
            }
            for offset in 1..separators {
                if rest.as_bytes()[offset] == b'\\' {
                    log.suffix(
                        start + rest_start + offset,
                        INVALID_REVERSE_SOLIDUS,
                        false,
                    );
                }
            }
        }
        // Path backslashes are reverse solidi; query/fragment bytes are handled by the
        // component scanner as InvalidUrlUnit instead.
        let path_end = rest[separators..]
            .find(['?', '#'])
            .map(|offset| separators + offset)
            .unwrap_or(rest.len());
        for (offset, character) in rest[separators..path_end].char_indices() {
            if character == '\\' {
                log.suffix(
                    start + rest_start + separators + offset,
                    INVALID_REVERSE_SOLIDUS,
                    false,
                );
            }
        }
    }
    if let Some((authority_start, authority_end)) = authority_bounds_with_base(trimmed) {
        let authority = &trimmed[authority_start..authority_end];
        if let Some(at) = authority.rfind('@') {
            log.suffix(
                start + authority_start + at,
                INVALID_CREDENTIALS,
                false,
            );
        }
        append_file_drive_host_warning(start, trimmed, authority_start, authority_end, log);
        if special {
            append_ipv4_warnings(
                start,
                authority_start,
                authority_end,
                trimmed,
                network_relative,
                log,
            );
        }
    }
}

/// Adds lexbor's soft `FileInvalidWindowsDriveLetterHost` warning for `file://c:/x`.
fn append_file_drive_host_warning(
    start: usize,
    trimmed: &str,
    authority_start: usize,
    authority_end: usize,
    log: &mut WarningLog,
) {
    let Some(scheme_end) = trimmed.find(':') else {
        return;
    };
    if !scheme_of(trimmed).is_some_and(|scheme| scheme.eq_ignore_ascii_case("file"))
        || !trimmed[scheme_end + 1..].starts_with("//")
    {
        return;
    }
    let authority = &trimmed[authority_start..authority_end];
    let host_start = authority_start + authority.rfind('@').map_or(0, |at| at + 1);
    let host = &trimmed[host_start..authority_end];
    let bytes = host.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_alphabetic() {
        return;
    }
    if bytes[1] != b':' && bytes[1] != b'|' {
        return;
    }
    // lexbor logs the pipe before the drive-letter host warning in php-src's array order, so
    // append the drive warning first and let `parse` reverse the whole log.
    log.suffix(
        start + host_start,
        FILE_INVALID_WINDOWS_DRIVE_LETTER_HOST,
        false,
    );
    if bytes[1] == b'|' {
        log.suffix(start + host_start + 1, INVALID_URL_UNIT, false);
    }
}

/// Warns for non-decimal IPv4 parts, matching lexbor's diagnostics.
///
/// Decimal parts with a leading zero report the part-to-host suffix. A hexadecimal part
/// reports the path suffix for the first part and the part text for later parts; a value above
/// 255 additionally reports `Ipv4OutOfRangePart`.
fn append_ipv4_warnings(
    start: usize,
    authority_start: usize,
    authority_end: usize,
    trimmed: &str,
    network_relative: bool,
    log: &mut WarningLog,
) {
    let authority = &trimmed[authority_start..authority_end];
    let after_userinfo = authority
        .rsplit_once('@')
        .map_or(authority, |(_, rest)| rest);
    let host = after_userinfo.split(':').next().unwrap_or(after_userinfo);
    if host.starts_with('[') || host.is_empty() {
        return;
    }
    // lexbor accepts one trailing dot but reports the empty final part as a soft warning.
    if host.ends_with('.') && ipv4_parts_valid(&host[..host.len() - 1]) {
        log.owned(b"", IPV4_EMPTY_PART, false);
    }
    let parts = host.split('.').collect::<Vec<_>>();
    if parts.len() > 4 {
        return;
    }
    let host_offset = authority.len() - after_userinfo.len();
    let path_context = path_context_range(start, authority_end, trimmed);
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
            Some((
                start + authority_start + hex_parts[0].0,
                start + authority_end,
            ))
        } else {
            path_context
        };
        for (index, (part_offset, part, _)) in hex_parts.iter().enumerate() {
            let context = if index == 0 {
                first_context
            } else {
                Some((
                    start + authority_start + part_offset,
                    start + authority_start + part_offset + part.len(),
                ))
            };
            match context {
                Some((range_start, range_end)) => {
                    log.range(range_start, range_end, IPV4_NON_DECIMAL_PART, false)
                }
                None => log.owned(b"/", IPV4_NON_DECIMAL_PART, false),
            }
        }
        for (index, (part_offset, part, value)) in hex_parts.iter().enumerate() {
            if *value > 255 {
                let context = if index == 0 {
                    path_context
                } else {
                    Some((
                        start + authority_start + part_offset,
                        start + authority_start + part_offset + part.len(),
                    ))
                };
                match context {
                    Some((range_start, range_end)) => {
                        log.range(range_start, range_end, IPV4_OUT_OF_RANGE_PART, true)
                    }
                    None => log.owned(b"/", IPV4_OUT_OF_RANGE_PART, true),
                }
            }
        }
        return;
    }
    for (index, (part_offset, _)) in decimal_leading_zero.iter().enumerate() {
        // A network-relative reference has no written scheme, so lexbor names the path for
        // the first leading-zero part and only then falls back to the host suffix.
        if network_relative && index == 0 {
            match path_context {
                Some((range_start, range_end)) => {
                    log.range(range_start, range_end, IPV4_NON_DECIMAL_PART, false)
                }
                None => log.owned(b"/", IPV4_NON_DECIMAL_PART, false),
            }
            continue;
        }
        log.range(
            start + authority_start + part_offset,
            start + authority_end,
            IPV4_NON_DECIMAL_PART,
            false,
        );
    }
}

/// Returns whether a host's dotted parts all parse under lexbor's IPv4 number rules.
fn ipv4_parts_valid(host: &str) -> bool {
    let parts = host.split('.').collect::<Vec<_>>();
    (1..=4).contains(&parts.len())
        && parts
            .iter()
            .all(|part| parse_ipv4_part(part.as_bytes()).is_some())
}

/// Returns the serialized path range for an IPv4 host, or `None` when the input omits it.
fn path_context_range(
    start: usize,
    authority_end: usize,
    trimmed: &str,
) -> Option<(usize, usize)> {
    let bytes = trimmed.as_bytes();
    if bytes.get(authority_end) != Some(&b'/') {
        return None;
    }
    let path_end = trimmed[authority_end..]
        .find(['?', '#'])
        .map(|offset| authority_end + offset)
        .unwrap_or(trimmed.len());
    Some((start + authority_end, start + path_end))
}

/// Returns the authority span of an absolute URL or a network-relative reference.
///
/// `//host/...` and `\\\\host\\...` carry an authority without a literal scheme, so the
/// credential and host warning scans must locate it even though `authority_bounds` starts
/// from the scheme colon.
fn authority_bounds_with_base(trimmed: &str) -> Option<(usize, usize)> {
    if trimmed.starts_with("//") || trimmed.starts_with("\\\\") {
        let authority_start = 2;
        let authority_end = trimmed[authority_start..]
            .find(['/', '\\', '?', '#'])
            .map(|offset| authority_start + offset)
            .unwrap_or(trimmed.len());
        return Some((authority_start, authority_end));
    }
    authority_bounds(trimmed)
}

/// Returns the `[scheme, authority)` bounds of a parsed URL, relative to the trimmed input.
fn authority_bounds(trimmed: &str) -> Option<(usize, usize)> {
    let scheme_end = trimmed.find(':')?;
    let scheme = &trimmed[..scheme_end];
    let rest = &trimmed[scheme_end + 1..];
    let special = is_special(scheme);
    let is_file = scheme.eq_ignore_ascii_case("file");
    let authority_start = if special && !is_file {
        // Special non-file schemes accept any run of solidi/reverse solidi before the
        // authority (`https:///host`, `https:host`). Skip the whole run so the host and
        // credential bounds describe the authority the parser used.
        let separators = rest
            .bytes()
            .take_while(|byte| matches!(byte, b'/' | b'\\'))
            .count();
        scheme_end + 1 + separators
    } else if rest.starts_with("//") {
        scheme_end + 3
    } else {
        return None;
    };
    // A special scheme treats a backslash as a path separator, so text after it is not part
    // of the authority (and an `@` there is not userinfo). Non-special hosts keep backslashes
    // as opaque host bytes.
    let authority_end = trimmed[authority_start..]
        .find(|character| {
            matches!(character, '/' | '?' | '#') || (special && character == '\\')
        })
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

/// Appends the failure entries (in lexbor log order) for one `url` parser error.
///
/// lexbor stores a pointer into the parser input for every entry and php-src stringifies it as
/// a C string, so a context is the input suffix starting at the failing pointer. The `url`
/// crate only reports a coarse error kind, so each branch re-derives lexbor's pointer from the
/// same component bounds the parser used.
fn failure_log(
    input: &str,
    (start, end): (usize, usize),
    base: Option<&Url>,
    error: ParseError,
    log: &mut WarningLog,
) {
    let trimmed = &input[start..end];
    let base_scheme = base.map(|url| url.scheme());
    // Separator diagnostics describe the reference text, so they survive a parse that fails
    // in the authority. Append them before the failure entries, which reverse to the front.
    append_special_separator_warnings(input, (start, end), base_scheme, log);
    match error {
        ParseError::RelativeUrlWithoutBase => {
            log.range(start, end, MISSING_SCHEME_NON_RELATIVE_URL, true)
        }
        ParseError::RelativeUrlWithCannotBeABaseBase => {
            log.range(start, end, INVALID_URL_UNIT, true)
        }
        ParseError::EmptyHost => host_missing_log(input, start, trimmed, base_scheme, log),
        ParseError::InvalidPort => port_failure_log(input, start, trimmed, base_scheme, log),
        ParseError::InvalidIpv4Address => match ipv4_failure_log(trimmed, base_scheme) {
            Some(entries) => log.extend(entries),
            None => {
                let code = ipv4_error_code(trimmed);
                log.range(start, end, code, code != IPV4_EMPTY_PART);
            }
        },
        ParseError::InvalidIpv6Address => {
            if !ipv6_failure_log(input, start, trimmed, base_scheme, log) {
                log.range(start, end, ipv6_error_code(trimmed), true);
            }
        }
        ParseError::InvalidDomainCharacter => {
            let special = scheme_of(trimmed)
                .map(is_special)
                .or_else(|| base_scheme.map(is_special))
                .unwrap_or(false);
            domain_failure_log(input, start, trimmed, special, base_scheme, log);
        }
        ParseError::IdnaError => {}
        ParseError::SetHostOnCannotBeABaseUrl => {
            host_missing_log(input, start, trimmed, base_scheme, log)
        }
        ParseError::Overflow => log.range(start, end, INVALID_URL_UNIT, true),
        _ => log.range(start, end, INVALID_URL_UNIT, true),
    }
}

/// Returns host bounds for an absolute URL or a network-relative reference resolved against
/// `base_scheme`. A network-relative reference has no written scheme, but its authority and
/// `file:` host rules still come from the reference text and the inherited base scheme.
fn host_bounds_with_base(trimmed: &str, base_scheme: Option<&str>) -> Option<(usize, usize)> {
    let (authority_start, authority_end) = authority_bounds_with_base(trimmed)?;
    let authority = &trimmed[authority_start..authority_end];
    let host_start = authority_start + authority.rfind('@').map_or(0, |at| at + 1);
    let rest = &trimmed[host_start..authority_end];
    let is_file = scheme_of(trimmed)
        .or(base_scheme)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file"));
    let host_len = if rest.starts_with('[') {
        rest.find(']').map_or(rest.len(), |close| close + 1)
    } else if is_file {
        rest.len()
    } else {
        rest.find(':').unwrap_or(rest.len())
    };
    Some((host_start, host_start + host_len))
}

/// Builds the host-missing log, paired with the soft credentials entry lexbor keeps.
fn host_missing_log(
    input: &str,
    start: usize,
    trimmed: &str,
    base_scheme: Option<&str>,
    log: &mut WarningLog,
) {
    let (authority_start, authority_end) =
        authority_bounds_with_base(trimmed).unwrap_or((0, 0));
    let authority = &trimmed[authority_start..authority_end];
    let userinfo = authority.rfind('@');
    if let Some(at) = userinfo {
        log.suffix(
            start + authority_start + at,
            INVALID_CREDENTIALS,
            false,
        );
    }
    let (host_start, _) = host_bounds_with_base(trimmed, base_scheme)
        .unwrap_or((authority_end, authority_end));
    let extra_solidus = userinfo.is_none()
        && trimmed[host_start..].starts_with(['/', '\\']);
    if extra_solidus {
        log.owned(b"", HOST_MISSING, true);
    } else {
        log.suffix(
            host_missing_context(input, start, trimmed, base_scheme),
            HOST_MISSING,
            true,
        );
    }
}

/// Returns the byte offset of the host-missing error pointer lexbor reports.
fn host_missing_context(
    input: &str,
    start: usize,
    trimmed: &str,
    base_scheme: Option<&str>,
) -> usize {
    match host_bounds_with_base(trimmed, base_scheme)
        .or_else(|| authority_bounds_with_base(trimmed).map(|(start, _)| (start, start)))
    {
        Some((host_start, _)) => start + host_start,
        None => start.min(input.len()),
    }
}

/// Percent-decodes and ASCII-lowercases a special-scheme host for domain diagnostics.
fn decoded_host(raw: &str) -> Vec<u8> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2])) {
                out.push(high << 4 | low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index].to_ascii_lowercase());
        index += 1;
    }
    out
}

/// Returns whether a byte is one of WHATWG's forbidden domain code points.
fn is_forbidden_domain_cp(byte: u8) -> bool {
    byte <= 0x20
        || byte == 0x7f
        || matches!(
            byte,
            b'#' | b'/' | b':' | b'<' | b'>' | b'?' | b'@' | b'[' | b'\\' | b']' | b'^' | b'|'
        )
}

/// Returns whether a byte is one of WHATWG's forbidden host code points.
fn is_forbidden_host_cp(byte: u8) -> bool {
    matches!(
        byte,
        0 | b'\t' | b'\n' | b'\r' | b' ' | b'#' | b'/' | b':' | b'<' | b'>' | b'?' | b'@'
            | b'[' | b'\\' | b']' | b'^' | b'|'
    )
}

/// Builds lexbor's domain/host failure entry at the first forbidden code point, if any.
fn forbidden_host_failure(
    input: &str,
    start: usize,
    trimmed: &str,
    special: bool,
    base_scheme: Option<&str>,
) -> Option<ValidationError> {
    let (host_start, host_end) = host_bounds_with_base(trimmed, base_scheme)?;
    let raw = &trimmed[host_start..host_end];
    if special {
        let decoded = decoded_host(raw);
        let offset = decoded.iter().position(|byte| is_forbidden_domain_cp(*byte))?;
        return Some(entry(&decoded[offset..], DOMAIN_INVALID_CODE_POINT, true));
    }
    let offset = raw.bytes().position(is_forbidden_host_cp)?;
    Some(entry(
        &input.as_bytes()[(start + host_start + offset).min(input.len())..],
        HOST_INVALID_CODE_POINT,
        true,
    ))
}

/// Builds lexbor's domain/host failure entry, falling back to the whole input.
fn domain_failure_log(
    input: &str,
    start: usize,
    trimmed: &str,
    special: bool,
    base_scheme: Option<&str>,
    log: &mut WarningLog,
) {
    match forbidden_host_failure(input, start, trimmed, special, base_scheme) {
        Some(failure) => log.entries.push(failure),
        None => log.suffix(start, DOMAIN_INVALID_CODE_POINT, true),
    }
}

/// Builds lexbor's port failure log, including the soft credentials entry it pairs with it.
fn port_failure_log(
    input: &str,
    start: usize,
    trimmed: &str,
    base_scheme: Option<&str>,
    log: &mut WarningLog,
) {
    let mut credentials = None;
    if let Some((authority_start, authority_end)) = authority_bounds_with_base(trimmed) {
        let authority = &trimmed[authority_start..authority_end];
        if let Some(at) = authority.rfind('@') {
            credentials = Some(start + authority_start + at);
        }
    }

    let special = scheme_of(trimmed)
        .map(is_special)
        .or_else(|| base_scheme.map(is_special))
        .unwrap_or(false);
    let (_, host_end) = host_bounds_with_base(trimmed, base_scheme).unwrap_or((0, 0));
    let mut failure = None;
    if trimmed[host_end..].starts_with(':') {
        let port_start = host_end + 1;
        let mut delimiters = vec!['/', '?', '#'];
        if special {
            delimiters.push('\\');
        }
        let port_end = trimmed[port_start..]
            .find(|character| delimiters.contains(&character))
            .map_or(trimmed.len(), |offset| port_start + offset);
        let port = &trimmed[port_start..port_end];
        let digits = port.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            if !port.is_empty() {
                failure = Some((start + port_start, PORT_INVALID, true));
            }
        } else if digits < port.len() {
            failure = Some((start + port_start + digits, PORT_INVALID, true));
        } else if port.parse::<u64>().map_or(true, |value| value > u64::from(u16::MAX)) {
            failure = Some((start + port_end, PORT_OUT_OF_RANGE, true));
        }
    }

    // lexbor appends the port failure after the credentials warning; `parse` reverses the log.
    if let Some(credentials) = credentials {
        log.suffix(credentials, INVALID_CREDENTIALS, false);
    }
    if let Some((offset, code, failure)) = failure {
        log.suffix(offset, code, failure);
    }
    if credentials.is_none() && failure.is_none() {
        log.suffix(start.min(input.len()), PORT_INVALID, true);
    }
}

/// Appends the IPv6 entry lexbor reports for a rejected bracketed host, returning whether one
/// was produced.
fn ipv6_failure_log(
    input: &str,
    start: usize,
    trimmed: &str,
    base_scheme: Option<&str>,
    log: &mut WarningLog,
) -> bool {
    let Some((host_start, host_end)) = host_bounds_with_base(trimmed, base_scheme) else {
        return false;
    };
    let host = &trimmed[host_start..host_end];
    if !host.starts_with('[') {
        return false;
    }
    if !host.ends_with(']') {
        let Some(offset) = host_end.checked_sub(1) else {
            return false;
        };
        log.suffix((start + offset).min(input.len()), IPV6_UNCLOSED, true);
        return true;
    }
    let content = &host[1..host.len() - 1];
    let Some((code, offset)) = scan_ipv6_error(content.as_bytes()) else {
        return false;
    };
    log.suffix(
        (start + host_start + 1 + offset).min(input.len()),
        code,
        true,
    );
    true
}

/// Scans an uncompressed/bracketed IPv6 body and returns lexbor's failure kind and pointer.
///
/// The scan mirrors the WHATWG IPv6 parser state machine, including the IPv4-in-IPv6 tail,
/// because the `url` crate collapses every malformed body to one error kind.
fn scan_ipv6_error(content: &[u8]) -> Option<(i64, usize)> {
    let end = content.len();
    let mut p = 0;
    let mut piece = 0;
    let mut compress = None;
    if p < end && content[p] == b':' {
        if p + 1 >= end || content[p + 1] != b':' {
            p = if p + 1 >= end { p } else { p + 1 };
            return Some((IPV6_INVALID_COMPRESSION, p));
        }
        p += 2;
        piece += 1;
        compress = Some(piece);
    }
    while p < end {
        if piece == 8 {
            return Some((IPV6_TOO_MANY_PIECES, p));
        }
        if content[p] == b':' {
            if compress.is_some() {
                return Some((IPV6_MULTIPLE_COMPRESSION, p));
            }
            p += 1;
            piece += 1;
            compress = Some(piece);
            continue;
        }
        let mut digits = 0;
        while digits < 4 && p < end && hex_value(content[p]).is_some() {
            p += 1;
            digits += 1;
        }
        if p >= end {
            piece += 1;
            break;
        }
        if content[p] == b'.' {
            if digits == 0 {
                return Some((IPV4_IN_IPV6_INVALID_CODE_POINT, p));
            }
            p -= digits;
            if piece > 6 {
                return Some((IPV4_IN_IPV6_TOO_MANY_PIECES, p));
            }
            match scan_ipv4_in_ipv6(content, &mut p, &mut piece) {
                Ok(()) => break,
                Err((code, offset)) => return Some((code, offset)),
            }
        }
        if content[p] == b':' {
            p += 1;
            if p >= end {
                return Some((IPV6_INVALID_CODE_POINT, p));
            }
        } else {
            return Some((IPV6_INVALID_CODE_POINT, p));
        }
        piece += 1;
    }
    if compress.is_none() && piece != 8 {
        return Some((IPV6_TOO_FEW_PIECES, p));
    }
    None
}

/// Scans the dotted IPv4 tail of an IPv6 address, matching lexbor's error positions.
fn scan_ipv4_in_ipv6(
    content: &[u8],
    p: &mut usize,
    piece: &mut usize,
) -> Result<(), (i64, usize)> {
    let end = content.len();
    let mut seen = 0;
    while *p < end {
        let mut value: i32 = -1;
        if seen > 0 {
            if content[*p] == b'.' && seen < 4 {
                *p += 1;
                if *p >= end {
                    break;
                }
            } else {
                return Err((IPV4_IN_IPV6_INVALID_CODE_POINT, *p));
            }
        }
        loop {
            let byte = content[*p];
            if !byte.is_ascii_digit() {
                if value == -1 {
                    return Err((IPV4_IN_IPV6_INVALID_CODE_POINT, *p));
                }
                break;
            }
            if value == -1 {
                value = i32::from(byte - b'0');
            } else if value == 0 {
                return Err((IPV4_IN_IPV6_INVALID_CODE_POINT, *p));
            } else {
                value = value * 10 + i32::from(byte - b'0');
            }
            if value > 255 {
                return Err((IPV4_IN_IPV6_OUT_OF_RANGE_PART, *p));
            }
            *p += 1;
            if *p >= end {
                break;
            }
        }
        seen += 1;
        if seen == 2 || seen == 4 {
            *piece += 1;
        }
    }
    if seen != 4 {
        return Err((IPV4_IN_IPV6_TOO_FEW_PARTS, *p));
    }
    Ok(())
}

/// Builds the IPv4 entry lexbor reports for a rejected dotted host.
fn ipv4_failure_log(
    trimmed: &str,
    base_scheme: Option<&str>,
) -> Option<Vec<ValidationError>> {
    let (host_start, host_end) = host_bounds_with_base(trimmed, base_scheme)?;
    let raw = &trimmed[host_start..host_end];
    if raw.starts_with('[') || raw.is_empty() {
        return None;
    }
    let host = decoded_host(raw);
    let mut entries = Vec::new();
    let mut index = 0;
    let mut begin = 0;
    let mut parts = 0;
    while index < host.len() {
        if host[index] == b'.' {
            if parts == 4 {
                entries.push(entry(&host[begin..], IPV4_TOO_MANY_PARTS, true));
                return Some(entries);
            }
            match push_ipv4_part(&host, begin, index, &mut entries) {
                Ok(()) => {}
                Err(()) => return Some(entries),
            }
            parts += 1;
            index += 1;
            begin = index;
            continue;
        }
        index += 1;
    }
    if begin < index {
        if parts == 4 {
            entries.push(entry(&host[begin..], IPV4_TOO_MANY_PARTS, true));
            return Some(entries);
        }
        let _ = push_ipv4_part(&host, begin, index, &mut entries);
    } else if index > 0 && host[index - 1] == b'.' {
        entries.push(entry(b"", IPV4_EMPTY_PART, false));
    }
    Some(entries)
}

/// Appends the soft non-decimal and fatal out-of-range entries for one IPv4 part.
fn push_ipv4_part(
    host: &[u8],
    begin: usize,
    end: usize,
    entries: &mut Vec<ValidationError>,
) -> Result<(), ()> {
    let (value, non_decimal) = match parse_ipv4_part(&host[begin..end]) {
        Some(parsed) => parsed,
        None => {
            entries.push(entry(&host[begin..], IPV4_NON_NUMERIC_PART, true));
            return Err(());
        }
    };
    if non_decimal {
        entries.push(entry(&host[begin..], IPV4_NON_DECIMAL_PART, false));
    }
    if value > 255 {
        entries.push(entry(&host[begin..], IPV4_OUT_OF_RANGE_PART, true));
    }
    Ok(())
}

/// Parses one IPv4 part with lexbor's decimal/hex/octal rules.
fn parse_ipv4_part(part: &[u8]) -> Option<(u64, bool)> {
    if part.is_empty() {
        return None;
    }
    let mut index = 0;
    let mut radix = 10u64;
    let mut non_decimal = false;
    if part.len() >= 2 && part[0] == b'0' {
        if part[1] == b'x' || part[1] == b'X' {
            index = 2;
            radix = 16;
        } else {
            index = 1;
            radix = 8;
        }
        non_decimal = true;
        if index >= part.len() {
            return Some((0, non_decimal));
        }
    }
    let mut value = 0u64;
    while index < part.len() {
        let digit = match (radix, part[index]) {
            (16, byte) => hex_value(byte)? as u64,
            (8, byte @ b'0'..=b'7') => u64::from(byte - b'0'),
            (10, byte @ b'0'..=b'9') => u64::from(byte - b'0'),
            _ => return None,
        };
        value = value.saturating_mul(radix).saturating_add(digit);
        index += 1;
    }
    Some((value, non_decimal))
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

/// Extracts the authority host text (without userinfo or port) from a raw URL, including a
/// network-relative reference with no written scheme.
fn host_text(input: &str) -> Option<&str> {
    let rest = if let Some(rest) = input
        .strip_prefix("//")
        .or_else(|| input.strip_prefix("\\\\"))
    {
        rest
    } else {
        let scheme_end = input.find(':')?;
        input.get(scheme_end + 1..)?
    };
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    if host_port.starts_with('[') {
        return host_port.split(']').next().map(|text| &text[1..]);
    }
    Some(host_port.split(':').next().unwrap_or(host_port))
}

/// Returns the scheme of a raw URL, when one is present.
///
/// Only a syntactically valid scheme counts: a colon inside a relative path or an authority
/// (`/x:y`, `//user@example.com:443/`) is not a scheme separator, and treating it as one
/// suppressed every base-scheme warning those references inherit.
fn scheme_of(input: &str) -> Option<&str> {
    let end = scheme_prefix_end(input)?;
    input.get(..end - 1)
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
                self.file_path
                    .clone()
                    .unwrap_or_else(|| self.url.path().to_string()),
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
        // The per-segment node estimate plus the path's real retained bytes: a single huge
        // segment must cost more than `/a`, because the native value keeps every byte.
        let path_units = 4 * segments + 2 * non_empty
            + crate::charge::string_charge_units(path);
        let scheme = crate::charge::string_charge_units(self.url.scheme());
        let username = if self.url.username().is_empty() {
            0
        } else {
            crate::charge::string_charge_units(self.url.username())
        };
        let password = crate::charge::optional_string_charge_units(self.url.password());
        let host = crate::charge::optional_string_charge_units(self.url.host_str());
        let query = crate::charge::optional_string_charge_units(self.url.query());
        let fragment = crate::charge::optional_string_charge_units(self.url.fragment());
        let long_host = self.long_host.as_ref().map_or(0, |(ascii, unicode)| {
            crate::charge::string_charge_units(ascii)
                .saturating_add(crate::charge::string_charge_units(unicode))
        });
        let file_path = crate::charge::optional_string_charge_units(self.file_path.as_deref());
        path_units
            .saturating_add(scheme)
            .saturating_add(username)
            .saturating_add(password)
            .saturating_add(host)
            .saturating_add(query)
            .saturating_add(fragment)
            .saturating_add(long_host)
            .saturating_add(file_path)
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
            // The `url` crate normalizes IPv4-mapped IPv6 spellings (`::ffff:1.2.3.4` ->
            // `::ffff:102:304`); reusing its host text keeps Unicode and ASCII in step.
            Some(Host::Ipv6(_)) => self.url.host_str().map(str::to_string),
            None => Some(String::new()),
        }
    }

    /// Serializes the URL in ASCII or Unicode mode, optionally excluding the fragment.
    pub fn try_to_string(&self, unicode: bool, exclude_fragment: bool) -> Option<String> {
        // The pre-allocation guard is exact only when the serialized URL is returned
        // unchanged. Unicode hosts, the long-host/file-path overrides and fragment
        // exclusion can all shorten or lengthen the result, so those modes rely on the
        // final check instead of rejecting a URL whose requested output still fits.
        if !unicode
            && !exclude_fragment
            && self.long_host.is_none()
            && self.file_path.is_none()
            && !crate::charge::within_php_uri_len(self.url.as_str().len())
        {
            return None;
        }
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
            text = replace_serialized_path(&text, path);
        }
        let text = strip_fragment(text, exclude_fragment);
        crate::charge::within_php_uri_len(text.len()).then_some(text)
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
        match (
            self.try_to_string(false, !include_fragment),
            other.try_to_string(false, !include_fragment),
        ) {
            (Some(left), Some(right)) => left == right,
            _ => false,
        }
    }

    /// Writes one string-or-null component into a clone of this URL.
    pub fn with_component(&self, component: i64, value: Option<&str>) -> Result<WhatWgUri, UriError> {
        let mut next = self.clone();
        match component {
            COMPONENT_SCHEME => {
                let previous_path = next.url.path().to_string();
                write_scheme(&mut next.url, value.unwrap_or(""))?;
                // A no-op or rejected scheme write must not discard the saved path; an
                // unrelated write used to change which file `file:////x` names. The saved
                // path survives a real scheme change too, as php-src keeps the component.
                if next.url.path() != previous_path {
                    next.file_path = None;
                }
            }
            COMPONENT_USERNAME => write_username(&mut next.url, value.unwrap_or("")),
            COMPONENT_PASSWORD => write_password(&mut next.url, value.unwrap_or("")),
            COMPONENT_HOST => {
                // Host writes never alter the path component; the saved path is independent.
                next.long_host = write_host(&mut next.url, value)?;
            }
            COMPONENT_PATH => {
                // PHP leaves an opaque (cannot-be-a-base) path unchanged, and lexbor keeps
                // the path of a URL parsed from a relative Windows-drive reference frozen.
                if !next.path_write_locked && !next.url.cannot_be_a_base() {
                    next.url.set_path(value.unwrap_or(""));
                    normalize_path_encoding(&mut next.url);
                    next.file_path = None;
                }
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
        // An opaque path keeps every space literal, including the one before a query or
        // fragment (`foo:a b ?c`). The `url` crate has already stripped a genuine
        // end-of-input trailing space.
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

/// Parses a URL whose host the strict `idna` crate rejects but lexbor accepts.
///
/// Two lexbor behaviours need this fallback: labels longer than the `idna` crate's 1000-code-
/// point cap are punycoded directly, and an ASCII `xn--` label whose Punycode payload decodes
/// to a valid label is re-normalized (lexbor runs UTS #46 ToASCII on the decoded label, so
/// `xn--a-.com` becomes `a.com`). A payload that does not decode stays invalid. Only absolute
/// inputs reach this path: `raw_host_bounds` needs a `://`, so a supplied base is irrelevant
/// and an absolute reference resolves exactly as it does without one.
fn parse_relaxed_idna_host(input: &str) -> Option<(Url, Option<(String, String)>)> {
    let (host_start, host_end) = raw_host_bounds(input)?;
    let host = &input[host_start..host_end];
    if host.starts_with('[') {
        return None;
    }
    let host_only = host.rsplit_once(':').map_or(host, |(host, _)| host);
    // lexbor percent-decodes a special-scheme host before UTS #46 runs, so an escaped label
    // (`c%6Fm`, `%78n--a-.com`) must be decoded before the relaxed recovery splits it.
    let decoded_host = String::from_utf8(decoded_host(host_only)).ok()?;
    let mut changed = decoded_host != host_only;
    // A single trailing dot is DNS's root anchor and lexbor preserves it. Strip it only for
    // label parsing, then restore it so an otherwise accepted host does not fail as an empty
    // final label; any empty internal label still rejects the recovery.
    let (labels_source, trailing_dot) = decoded_host
        .strip_suffix('.')
        .map_or((decoded_host.as_str(), false), |core| (core, true));
    let (raw_labels_source, _) = host_only
        .strip_suffix('.')
        .map_or((host_only, false), |core| (core, true));
    let raw_labels = raw_labels_source.split('.').collect::<Vec<_>>();
    let decoded_labels = labels_source.split('.').collect::<Vec<_>>();
    // lexbor percent-decodes a host but does NOT re-normalize an A-label whose `xn--` prefix
    // itself carried escapes: `%78n--a-.com` keeps the ASCII host `xn--a-.com` while its
    // Unicode form decodes to `a.com`. Keep the raw escaped label for the ASCII reconstruction
    // in that case; the ordinary parser decodes it without re-running Punycode.
    let escaped_prefix_labels = (raw_labels.len() == decoded_labels.len())
        .then(|| {
            raw_labels
                .iter()
                .zip(decoded_labels.iter())
                .enumerate()
                .filter(|(_, (raw, decoded))| {
                    raw.bytes()
                        .take(4)
                        .any(|byte| byte == b'%')
                        && decoded
                            .get(..4)
                            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("xn--"))
                })
                .map(|(index, _)| index)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let preserve_ascii_punycode = !escaped_prefix_labels.is_empty();
    let mut ascii_labels = Vec::new();
    let mut unicode_labels = Vec::new();
    for (index, label) in labels_source.split('.').enumerate() {
        if escaped_prefix_labels.contains(&index) {
            // Keep the decoded A-label as the ASCII host, but expose the decoded Unicode
            // label through the saved override. Re-parsing the literal A-label here would
            // normalize it to its Unicode form and lose lexbor's ASCII spelling.
            let unicode = punycode::decode(&label[4..]).unwrap_or_else(|_| label.to_string());
            ascii_labels.push(label.to_ascii_lowercase());
            unicode_labels.push(unicode);
            changed = true;
            continue;
        }
        let decoded = label
            .get(..4)
            .filter(|prefix| prefix.eq_ignore_ascii_case("xn--"))
            .and_then(|_| punycode::decode(&label[4..]).ok());
        let candidate = decoded.as_deref().unwrap_or(label);
        if candidate.is_ascii() {
            // Lexbor preserves empty labels (`a..com`, `.`) and only an empty HOST is invalid;
            // every empty label here is re-parsed by the ordinary WHATWG host parser below.
            if host_has_forbidden_domain_code_point(candidate) {
                return None;
            }
            let lower = candidate.to_ascii_lowercase();
            ascii_labels.push(lower.clone());
            unicode_labels.push(lower);
            changed |= decoded.is_some();
            continue;
        }
        let encoded = match idna::domain_to_ascii(candidate) {
            Ok(encoded) => encoded,
            // The `idna` crate deliberately caps labels at 1000 code points; beyond that,
            // punycode-encode directly. A shorter invalid label stays invalid.
            Err(_) if candidate.chars().count() > 1000 => {
                punycode::encode(candidate).map(|encoded| format!("xn--{encoded}")).ok()?
            }
            Err(_) => return None,
        };
        changed = true;
        ascii_labels.push(encoded);
        unicode_labels.push(candidate.to_string());
    }
    if !changed {
        return None;
    }
    let mut ascii_host = ascii_labels.join(".");
    let mut unicode_host = unicode_labels.join(".");
    if trailing_dot {
        ascii_host.push('.');
        unicode_host.push('.');
    }
    if host_has_forbidden_domain_code_point(&ascii_host) {
        return None;
    }
    // First let the ordinary WHATWG host parser consume the recovered ASCII host. This runs
    // numeric hosts through IPv4 parsing (`0x7f.1` -> `127.0.0.1`) and rejects out-of-range
    // values; only hosts it still cannot parse need the placeholder + saved override.
    let recovered = format!(
        "{}{}{}",
        &input[..host_start],
        ascii_host,
        &input[host_start + host_only.len()..]
    );
    if !preserve_ascii_punycode {
        match Url::parse(&recovered) {
            Ok(url) => return Some((url, None)),
            Err(ParseError::IdnaError) => {}
            Err(_) => return None,
        }
    }
    let replaced = format!(
        "{}elephc.invalid{}",
        &input[..host_start],
        &input[host_start + host_only.len()..]
    );
    let url = Url::parse(&replaced).ok()?;
    Some((url, Some((ascii_host, unicode_host))))
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

/// Returns whether `reference` is a relative Windows-drive reference (`c|/x`, `c:/x`).
///
/// lexbor's file-URL path setter is inert after such a resolution, unlike after an absolute
/// `file:` URL. The reference has no scheme of its own; a real `c:` scheme is excluded by
/// `scheme_prefix_end`.
fn relative_windows_drive_reference(reference: &str) -> bool {
    if scheme_prefix_end(reference).is_some() {
        return false;
    }
    let bytes = reference.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && matches!(bytes[1], b'|' | b':')
        && (bytes.len() == 2 || matches!(bytes[2], b'/' | b'\\'))
}

/// Returns the lexbor path for an empty-host `file:` URL with more than three solidi.
fn file_path_override(trimmed: &str, url: &Url) -> Option<String> {
    if url.scheme() != "file" || url.host_str().is_some_and(|host| !host.is_empty()) {
        return None;
    }
    // Only a syntactically valid scheme prefix selects the absolute-file override; a colon
    // inside a query, fragment, or relative path must not add path separators.
    let rest = &trimmed[scheme_prefix_end(trimmed)?..];
    let solidi = rest
        .bytes()
        .take_while(|byte| *byte == b'/' || *byte == b'\\')
        .count();
    if solidi <= 3 {
        return None;
    }
    Some(format!("{}{}", "/".repeat(solidi - 3), url.path()))
}

/// Returns lexbor's path for a RELATIVE `file:` reference with three or more solidi.
///
/// `Url::set_path` collapses the extra leading slashes the way it does for an absolute
/// `file:` URL, but lexbor keeps them (`////x` resolves to `file:////x`).
fn relative_file_path_override(reference: &str, url: &Url) -> Option<String> {
    if url.scheme() != "file" || url.host_str().is_some_and(|host| !host.is_empty()) {
        return None;
    }
    let slashes = reference
        .bytes()
        .take_while(|byte| matches!(byte, b'/' | b'\\'))
        .count();
    if slashes < 3 {
        return None;
    }
    Some(format!(
        "{}{}",
        "/".repeat(slashes - 2),
        url.path().trim_start_matches('/')
    ))
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

/// Replaces the serialized path while preserving the authority and query/fragment suffix.
///
/// The saved path of `file:////x` is `//x`, and php-src keeps it after `withHost()` or a
/// scheme change, so the replacement cannot be limited to the `file://` prefix.
fn replace_serialized_path(serialized: &str, path: &str) -> String {
    let Some(scheme_end) = serialized.find(':') else {
        return serialized.to_string();
    };
    let mut authority_start = scheme_end + 1;
    if serialized[authority_start..].starts_with("//") {
        authority_start += 2;
    }
    let path_start = serialized[authority_start..]
        .find(['/', '?', '#'])
        .map(|offset| authority_start + offset)
        .unwrap_or(serialized.len());
    let path_end = serialized[path_start..]
        .find(['?', '#'])
        .map(|offset| path_start + offset)
        .unwrap_or(serialized.len());
    let mut out = String::with_capacity(serialized.len() + path.len());
    out.push_str(&serialized[..path_start]);
    out.push_str(path);
    out.push_str(&serialized[path_end..]);
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
///
/// Returns the `(ascii, unicode)` override when lexbor's relaxed `xn--` recovery was needed.
fn write_host(
    url: &mut Url,
    value: Option<&str>,
) -> Result<Option<(String, String)>, UriError> {
    if url.cannot_be_a_base() {
        return Ok(None);
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
            Ok(None)
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
            Ok(None)
        }
        Some(value) => {
            let value = value.trim_matches(|character: char| character <= ' ');
            // lexbor treats a URL delimiter in a host setter as the end of the host.
            let value = value
                .split(['#', '?', '/', '\\'])
                .next()
                .unwrap_or(value);
            if value.is_empty() {
                if special && !is_file {
                    return Err(component_error("host", HOST_MISSING, b""));
                }
                if is_file {
                    let _ = url.set_host(None);
                } else {
                    let _ = url.set_host(Some(""));
                }
                return Ok(None);
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
            // A non-special host is opaque: `%2F`, `%40`, and `%3A` stay escapes, exactly as
            // they do when parsing `foo://exa%2Fmple.org/`. Only literal forbidden bytes
            // reject a host setter here.
            let literal_forbidden = !bracketed
                && value
                    .bytes()
                    .any(|byte| matches!(byte, b'@' | 0 | b'[' | b']' | b':'));
            if (special && forbidden) || (!special && literal_forbidden) {
                return Err(host_code_error(value, special));
            }
            match url.set_host(Some(value)) {
                Ok(()) => {
                    let host = url.host_str().unwrap_or("");
                    // Bracketed IPv6/IPvFuture hosts legitimately contain `:`, `[`, and `]`.
                    let forbidden = !host.starts_with('[')
                        && if special {
                            host_has_forbidden_domain_code_point(host)
                        } else {
                            host.bytes().any(is_forbidden_host_cp)
                        };
                    if forbidden {
                        Err(host_code_error(value, special))
                    } else {
                        Ok(None)
                    }
                }
                Err(ParseError::EmptyHost) => Err(component_error("host", HOST_MISSING, value.as_bytes())),
                Err(ParseError::IdnaError) => match recover_host_value(url, value) {
                    Some((recovered, long_host)) => {
                        *url = recovered;
                        Ok(long_host)
                    }
                    None => Err(host_code_error(value, special)),
                },
                Err(ParseError::InvalidDomainCharacter)
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

/// Re-parses a URL with a replacement host through lexbor's relaxed `xn--` recovery.
fn recover_host_value(url: &Url, value: &str) -> Option<(Url, Option<(String, String)>)> {
    let serialized = url.as_str();
    let mut candidate = replace_serialized_host(serialized, url, value);
    if candidate == serialized {
        candidate = insert_host_into_empty_authority(serialized, value)?;
    }
    parse_relaxed_idna_host(&candidate)
}

/// Inserts a host after an empty `scheme://` authority, preserving path and suffix.
fn insert_host_into_empty_authority(serialized: &str, value: &str) -> Option<String> {
    let scheme_end = serialized.find(':')?;
    let mut start = scheme_end + 1;
    if !serialized[start..].starts_with("//") {
        return None;
    }
    start += 2;
    let mut out = String::with_capacity(serialized.len() + value.len());
    out.push_str(&serialized[..start]);
    out.push_str(value);
    out.push_str(&serialized[start..]);
    Some(out)
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

#[cfg(test)]
mod tests {
    //! Focused bridge-level coverage for WHATWG resolution and mutation edge cases.

    use super::*;

    /// Resolves `reference` against an optional serialized base and returns the ASCII form.
    fn resolved(reference: &str, base: Option<&str>) -> String {
        let base = base.map(|input| parse(input, None).expect("base parses").0);
        let (url, _) = parse(reference, base.as_ref()).expect("reference resolves");
        url.try_to_string(false, false).expect("serialized URL")
    }

    /// Excluding the fragment shortens the serialized URL, so the full-URL length guard
    /// must not reject a URL whose fragment-free output still fits.
    #[test]
    fn fragment_exclusion_shortens_the_serialized_url() {
        let (url, _) = parse("https://example.com/path#fragment", None).expect("url parses");
        assert_eq!(
            url.try_to_string(false, false).as_deref(),
            Some("https://example.com/path#fragment")
        );
        assert_eq!(
            url.try_to_string(false, true).as_deref(),
            Some("https://example.com/path")
        );
    }

    /// Verifies extra solidi against a `file:` base stay path bytes instead of becoming a host.
    #[test]
    fn relative_file_extra_solidus_preserves_the_path() {
        assert_eq!(resolved("///tmp/x", Some("file:///base/")), "file:///tmp/x");
        assert_eq!(resolved("////x", Some("file:///base/")), "file:////x");
        assert_eq!(resolved("/////x", Some("file:///base/")), "file://///x");
    }

    /// Verifies non-special schemes keep backslashes literal in the path, query, and fragment,
    /// and leave existing `%5C` escapes untouched.
    #[test]
    fn non_special_backslashes_stay_literal_without_decoding_existing_escapes() {
        assert_eq!(
            resolved(r"/p?q\z#f\g", Some("foo://h/base")),
            r"foo://h/p?q\z#f\g"
        );
        assert_eq!(resolved("/%5Cx", Some("foo://h/base")), "foo://h/%5Cx");
    }

    /// Locks the `url` 2.5.9 parser changes behind php-src-compatible serialization.
    #[test]
    fn url_2_5_9_parser_changes_keep_php_whatwg_results() {
        assert_eq!(
            resolved("https://example.com/a^b", None),
            "https://example.com/a%5Eb"
        );
        assert_eq!(
            resolved(r"https://example.com:81\path", None),
            "https://example.com:81/path"
        );
        assert_eq!(resolved("file:///w|/m", None), "file:///w:/m");

        let parsed = parse("https://example.com/", None)
            .expect("base parses")
            .0;
        let changed = parsed
            .with_component(COMPONENT_PATH, Some("/a^b"))
            .expect("path write succeeds");
        assert_eq!(
            changed.try_to_string(false, false).as_deref(),
            Some("https://example.com/a%5Eb")
        );
    }

    /// Verifies a relative Windows-drive reference only freezes path writes on a `file:` base.
    #[test]
    fn https_relative_drive_keeps_with_path_writable() {
        let base = parse("https://example.com/", None).expect("base parses").0;
        let (url, _) = parse("c|/x", Some(&base)).expect("reference resolves");
        assert_eq!(url.try_to_string(false, false).expect("serialized URL"), "https://example.com/c|/x");
        let changed = url
            .with_component(COMPONENT_PATH, Some("/new"))
            .expect("path write succeeds");
        assert_eq!(changed.try_to_string(false, false).as_deref(), Some("https://example.com/new"));
    }
}
