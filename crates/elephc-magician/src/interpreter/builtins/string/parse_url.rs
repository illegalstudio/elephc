//! Purpose:
//! Declarative eval registry entry and PHP-compatible byte scanner for `parse_url`.
//!
//! Called from:
//! - Eval direct calls and evaluated callable dispatch through the `ParseUrl` hooks.
//!
//! Key details:
//! - The scanner follows PHP 8.4 `ext/standard/url.c`, including scheme-relative URLs,
//!   bracket-preserving IPv6 hosts, empty present components, and port validation.
//! - Control bytes in returned components are replaced with `_`, matching php-src.
//! - Any negative component selects the full array, as PHP 8.4 does; values above 7
//!   raise a catchable `ValueError` with PHP's exact message.

eval_builtin! {
    contract: "parse_url",
    area: String,
    direct: ParseUrl,
    values: ParseUrl,
}

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

/// Evaluates a direct `parse_url()` call while preserving PHP source argument order.
pub(in crate::interpreter) fn eval_builtin_parse_url(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [url] => {
            let url = eval_expr(url, context, scope, values)?;
            eval_parse_url_result(url, None, context, values)
        }
        [url, component] => {
            let url = eval_expr(url, context, scope, values)?;
            let component = eval_expr(component, context, scope, values)?;
            eval_parse_url_result(url, Some(component), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches a callable or named-argument `parse_url()` call after argument binding.
pub(in crate::interpreter) fn eval_parse_url_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [url] => eval_parse_url_result(*url, None, context, values),
        [url, component] => eval_parse_url_result(*url, Some(*component), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Parses one URL and materializes the requested PHP result shape.
pub(in crate::interpreter) fn eval_parse_url_result(
    url: RuntimeCellHandle,
    component: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let url = values.string_bytes(url)?;
    let component = match component {
        Some(component) => eval_int_value(component, values)?,
        None => -1,
    };
    let Some(parsed) = eval_parse_url_parts(&url) else {
        return values.bool_value(false);
    };
    if component > EVAL_PHP_URL_FRAGMENT {
        let message = format!(
            "parse_url(): Argument #2 ($component) must be a valid URL component identifier, {component} given"
        );
        return eval_throw_builtin_value_error(&message, context, values);
    }
    if component < 0 {
        return eval_parse_url_array_result(&parsed, values);
    }
    eval_parse_url_component_result(&parsed, component, values)
}

/// Builds the associative array form in PHP's stable component insertion order.
fn eval_parse_url_array_result(
    parsed: &EvalParsedUrl,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::assoc(values, 8)?;
    for (key, value) in [("scheme", &parsed.scheme), ("host", &parsed.host)] {
        if let Some(value) = value {
            result.string(key, |values| values.string_bytes_value(value))?;
        }
    }
    if let Some(port) = parsed.port {
        result.string("port", |values| values.int(i64::from(port)))?;
    }
    for (key, value) in [
        ("user", &parsed.user),
        ("pass", &parsed.pass),
        ("path", &parsed.path),
        ("query", &parsed.query),
        ("fragment", &parsed.fragment),
    ] {
        if let Some(value) = value {
            result.string(key, |values| values.string_bytes_value(value))?;
        }
    }
    Ok(result.finish())
}

/// Materializes one component as string, integer port, or null when absent.
fn eval_parse_url_component_result(
    parsed: &EvalParsedUrl,
    component: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if component == EVAL_PHP_URL_PORT {
        return match parsed.port {
            Some(port) => values.int(i64::from(port)),
            None => values.null(),
        };
    }
    let value = match component {
        EVAL_PHP_URL_SCHEME => parsed.scheme.as_deref(),
        EVAL_PHP_URL_HOST => parsed.host.as_deref(),
        EVAL_PHP_URL_USER => parsed.user.as_deref(),
        EVAL_PHP_URL_PASS => parsed.pass.as_deref(),
        EVAL_PHP_URL_PATH => parsed.path.as_deref(),
        EVAL_PHP_URL_QUERY => parsed.query.as_deref(),
        EVAL_PHP_URL_FRAGMENT => parsed.fragment.as_deref(),
        _ => None,
    };
    match value {
        Some(value) => values.string_bytes_value(value),
        None => values.null(),
    }
}

/// Parser control state mirroring php-src's `parse_port`, `parse_host`, and `just_path` labels.
#[derive(Clone, Copy)]
enum EvalParseUrlState {
    /// Reinterpret the colon at the stored offset as a host/port separator.
    Port(usize),
    /// Parse authority credentials, host, and optional port from the current cursor.
    Host,
    /// Parse path, query, and fragment from the current cursor.
    Path,
}

/// Parsed PHP URL parts with missing and present-empty strings kept distinct.
#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::interpreter) struct EvalParsedUrl {
    /// Scheme without its trailing colon.
    scheme: Option<Vec<u8>>,
    /// Authority host, retaining IPv6 brackets.
    host: Option<Vec<u8>>,
    /// Valid explicit port, including zero.
    port: Option<u16>,
    /// User-info name, possibly empty when a password is present.
    user: Option<Vec<u8>>,
    /// User-info password, possibly empty.
    pass: Option<Vec<u8>>,
    /// Path bytes, possibly empty only for the empty input.
    path: Option<Vec<u8>>,
    /// Raw query bytes without `?`, possibly empty.
    query: Option<Vec<u8>>,
    /// Raw fragment bytes without `#`, possibly empty.
    fragment: Option<Vec<u8>>,
}

/// Parses URL bytes using the decision tree from PHP 8.4 `php_url_parse_ex2`.
pub(in crate::interpreter) fn eval_parse_url_parts(url: &[u8]) -> Option<EvalParsedUrl> {
    let end = url.len();
    let mut cursor = 0usize;
    let mut parsed = EvalParsedUrl::default();
    let colon = url.iter().position(|byte| *byte == b':');
    let state = match colon {
        Some(colon) if colon != 0 => {
            let valid_scheme = url[..colon].iter().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(*byte, b'+' | b'-' | b'.')
            });
            if !valid_scheme {
                let first_query_or_fragment = eval_parse_url_first_delimiter(url, 0, b"?#");
                if colon + 1 < end && colon < first_query_or_fragment {
                    EvalParseUrlState::Port(colon)
                } else if url.starts_with(b"//") {
                    cursor = 2;
                    EvalParseUrlState::Host
                } else {
                    EvalParseUrlState::Path
                }
            } else if colon + 1 == end {
                parsed.scheme = Some(eval_parse_url_component(&url[..colon]));
                return Some(parsed);
            } else if url[colon + 1] != b'/' {
                let mut after = colon + 1;
                while after < end && url[after].is_ascii_digit() {
                    after += 1;
                }
                if (after == end || url[after] == b'/') && after - colon < 7 {
                    EvalParseUrlState::Port(colon)
                } else {
                    parsed.scheme = Some(eval_parse_url_component(&url[..colon]));
                    cursor = colon + 1;
                    EvalParseUrlState::Path
                }
            } else {
                parsed.scheme = Some(eval_parse_url_component(&url[..colon]));
                if colon + 2 < end && url[colon + 2] == b'/' {
                    cursor = colon + 3;
                    if parsed.scheme.as_deref().is_some_and(|scheme| {
                        scheme.eq_ignore_ascii_case(b"file")
                    }) && colon + 3 < end
                        && url[colon + 3] == b'/'
                    {
                        if colon + 5 < end && url[colon + 5] == b':' {
                            cursor = colon + 4;
                        }
                        EvalParseUrlState::Path
                    } else {
                        EvalParseUrlState::Host
                    }
                } else {
                    cursor = colon + 1;
                    EvalParseUrlState::Path
                }
            }
        }
        Some(colon) => EvalParseUrlState::Port(colon),
        None if url.starts_with(b"//") => {
            cursor = 2;
            EvalParseUrlState::Host
        }
        None => EvalParseUrlState::Path,
    };
    eval_parse_url_continue(url, cursor, parsed, state)
}

/// Continues the parser through port, authority, and path states until it returns.
fn eval_parse_url_continue(
    url: &[u8],
    mut cursor: usize,
    mut parsed: EvalParsedUrl,
    mut state: EvalParseUrlState,
) -> Option<EvalParsedUrl> {
    let end = url.len();
    loop {
        match state {
            EvalParseUrlState::Port(colon) => {
                let port_start = colon + 1;
                let mut port_end = port_start;
                while port_end < end
                    && port_end - port_start < 6
                    && url[port_end].is_ascii_digit()
                {
                    port_end += 1;
                }
                let digit_len = port_end - port_start;
                if digit_len > 0
                    && digit_len < 6
                    && (port_end == end || url[port_end] == b'/')
                {
                    parsed.port = eval_parse_url_port(&url[port_start..port_end]);
                    parsed.port?;
                    if url.starts_with(b"//") {
                        cursor = 2;
                    }
                    state = EvalParseUrlState::Host;
                } else if digit_len == 0 && port_end == end {
                    return None;
                } else if url.starts_with(b"//") {
                    cursor = 2;
                    state = EvalParseUrlState::Host;
                } else {
                    state = EvalParseUrlState::Path;
                }
            }
            EvalParseUrlState::Host => {
                let authority_end = eval_parse_url_first_delimiter(url, cursor, b"/?#");
                if let Some(at) = url[cursor..authority_end]
                    .iter()
                    .rposition(|byte| *byte == b'@')
                    .map(|offset| cursor + offset)
                {
                    if let Some(colon) = url[cursor..at]
                        .iter()
                        .position(|byte| *byte == b':')
                        .map(|offset| cursor + offset)
                    {
                        parsed.user = Some(eval_parse_url_component(&url[cursor..colon]));
                        parsed.pass = Some(eval_parse_url_component(&url[colon + 1..at]));
                    } else {
                        parsed.user = Some(eval_parse_url_component(&url[cursor..at]));
                    }
                    cursor = at + 1;
                }
                let bracketed_ipv6 = cursor < end
                    && cursor < authority_end
                    && url[cursor] == b'['
                    && url[authority_end - 1] == b']';
                let port_colon = if bracketed_ipv6 {
                    None
                } else {
                    url[cursor..authority_end]
                        .iter()
                        .rposition(|byte| *byte == b':')
                        .map(|offset| cursor + offset)
                };
                let host_end = if let Some(colon) = port_colon {
                    if parsed.port.is_none() {
                        let port_bytes = &url[colon + 1..authority_end];
                        if port_bytes.len() > 5 {
                            return None;
                        }
                        if !port_bytes.is_empty() {
                            parsed.port = eval_parse_url_port(port_bytes);
                            parsed.port?;
                        }
                    }
                    colon
                } else {
                    authority_end
                };
                if host_end <= cursor {
                    return None;
                }
                parsed.host = Some(eval_parse_url_component(&url[cursor..host_end]));
                if authority_end == end {
                    return Some(parsed);
                }
                cursor = authority_end;
                state = EvalParseUrlState::Path;
            }
            EvalParseUrlState::Path => {
                eval_parse_url_path_parts(url, cursor, &mut parsed);
                return Some(parsed);
            }
        }
    }
}

/// Parses PHP's whitespace-tolerant signed decimal port prefix and enforces `0..=65535`.
fn eval_parse_url_port(bytes: &[u8]) -> Option<u16> {
    let whitespace_len = bytes
        .iter()
        .take_while(|byte| matches!(**byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .count();
    let bytes = &bytes[whitespace_len..];
    let (negative, digits) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    let digit_count = digits.iter().take_while(|byte| byte.is_ascii_digit()).count();
    if digit_count == 0 {
        return None;
    }
    let mut value = 0_i64;
    for byte in &digits[..digit_count] {
        value = value * 10 + i64::from(*byte - b'0');
    }
    if negative {
        value = -value;
    }
    u16::try_from(value).ok()
}

/// Splits path, query, and fragment while preserving present-empty query/fragment values.
fn eval_parse_url_path_parts(url: &[u8], start: usize, parsed: &mut EvalParsedUrl) {
    let mut end = url.len();
    if let Some(fragment) = url[start..end]
        .iter()
        .position(|byte| *byte == b'#')
        .map(|offset| start + offset)
    {
        parsed.fragment = Some(eval_parse_url_component(&url[fragment + 1..end]));
        end = fragment;
    }
    if let Some(query) = url[start..end]
        .iter()
        .position(|byte| *byte == b'?')
        .map(|offset| start + offset)
    {
        parsed.query = Some(eval_parse_url_component(&url[query + 1..end]));
        end = query;
    }
    if start < end || start == url.len() {
        parsed.path = Some(eval_parse_url_component(&url[start..end]));
    }
}

/// Returns the first delimiter offset in `url[start..]`, or `url.len()` when absent.
fn eval_parse_url_first_delimiter(url: &[u8], start: usize, delimiters: &[u8]) -> usize {
    url[start..]
        .iter()
        .position(|byte| delimiters.contains(byte))
        .map_or(url.len(), |offset| start + offset)
}

/// Copies one component and replaces ASCII control bytes with PHP's `_` substitute.
fn eval_parse_url_component(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .map(|byte| if byte.is_ascii_control() { b'_' } else { *byte })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, Number, Value};

    /// Verifies the shared PHP-derived fixture corpus against the pure Rust scanner.
    #[test]
    fn parse_url_scanner_matches_shared_php_fixtures() {
        let cases: Value = serde_json::from_str(include_str!(
            "../../../../../../tests/fixtures/parse_url_cases.json"
        ))
        .expect("parse_url fixture JSON must parse");
        for case in cases.as_array().expect("fixture root must be an array") {
            let url = case["url"].as_str().expect("fixture URL must be a string");
            let actual = eval_parse_url_parts(url.as_bytes())
                .as_ref()
                .map(eval_parsed_url_json)
                .unwrap_or(Value::Bool(false));
            assert_eq!(actual, case["result"], "URL fixture {url:?}");
        }
    }

    /// Converts parsed parts into the JSON shape stored in the shared fixture file.
    fn eval_parsed_url_json(parsed: &EvalParsedUrl) -> Value {
        let mut result = Map::new();
        eval_insert_json_string(&mut result, "scheme", parsed.scheme.as_deref());
        eval_insert_json_string(&mut result, "host", parsed.host.as_deref());
        if let Some(port) = parsed.port {
            result.insert("port".to_string(), Value::Number(Number::from(port)));
        }
        eval_insert_json_string(&mut result, "user", parsed.user.as_deref());
        eval_insert_json_string(&mut result, "pass", parsed.pass.as_deref());
        eval_insert_json_string(&mut result, "path", parsed.path.as_deref());
        eval_insert_json_string(&mut result, "query", parsed.query.as_deref());
        eval_insert_json_string(&mut result, "fragment", parsed.fragment.as_deref());
        Value::Object(result)
    }

    /// Inserts one optional UTF-8 fixture component into a JSON object.
    fn eval_insert_json_string(result: &mut Map<String, Value>, key: &str, value: Option<&[u8]>) {
        if let Some(value) = value {
            result.insert(
                key.to_string(),
                Value::String(String::from_utf8_lossy(value).into_owned()),
            );
        }
    }
}
