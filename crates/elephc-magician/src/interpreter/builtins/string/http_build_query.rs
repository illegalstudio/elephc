//! Purpose:
//! Eval registry entry and implementation for `http_build_query`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`, through the URL encode hooks.
//!
//! Key details:
//! - Mirrors the compiled prelude (`src/http_build_query_prelude.rs`) and PHP 8: `null` and
//!   resource values are skipped, booleans render `1`/`0`, nested keys become
//!   `name%5Bkey%5D`, `$numeric_prefix` is prepended raw to top-level integer keys only,
//!   `$arg_separator` defaults to `&`, and `PHP_QUERY_RFC3986` (2) selects `rawurlencode`.
//! - Object values contribute what `get_object_vars()` returns from the current eval scope.
//! - Every cell read from an array is released once its bytes are copied.

eval_builtin! {
    contract: "http_build_query",
    area: String,
    direct: UrlEncode,
    values: UrlEncode,
}

use super::super::super::*;
use super::super::symbols::eval_get_object_vars_result;

/// `PHP_QUERY_RFC3986`: encode with `rawurlencode` rules.
const EVAL_PHP_QUERY_RFC3986: i64 = 2;

/// Evaluates PHP `http_build_query(...)` over eval argument expressions.
pub(in crate::interpreter) fn eval_builtin_http_build_query(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() || args.len() > 4 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_http_build_query_result(&evaluated, context, values)
}

/// Builds the query string from already evaluated `http_build_query()` arguments.
pub(in crate::interpreter) fn eval_http_build_query_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((data, rest)) = args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let numeric_prefix = match rest.first() {
        Some(prefix) => values.string_bytes(*prefix)?,
        None => Vec::new(),
    };
    let separator = match rest.get(1) {
        Some(separator) if values.type_tag(*separator)? != EVAL_TAG_NULL => {
            values.string_bytes(*separator)?
        }
        _ => b"&".to_vec(),
    };
    let raw = match rest.get(2) {
        Some(encoding) => eval_int_value(*encoding, values)? == EVAL_PHP_QUERY_RFC3986,
        None => false,
    };
    let query = EvalQuery { numeric_prefix, separator, raw };
    let mut out = Vec::new();
    match values.type_tag(*data)? {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            eval_http_build_query_level(*data, None, &query, &mut out, context, values)?;
        }
        EVAL_TAG_OBJECT => {
            let properties = eval_get_object_vars_result(&[*data], context, values)?;
            let built =
                eval_http_build_query_level(properties, None, &query, &mut out, context, values);
            values.release(properties)?;
            built?;
        }
        _ => {
            let given = eval_given_type_name(*data, context, values)?;
            return eval_throw_type_error(
                &format!(
                    "http_build_query(): Argument #1 ($data) must be of type array, {given} given"
                ),
                context,
                values,
            );
        }
    }
    values.string_bytes_value(&out)
}

/// The per-call settings shared by every nesting level.
struct EvalQuery {
    /// Raw prefix for top-level integer keys.
    numeric_prefix: Vec<u8>,
    /// Separator between pairs.
    separator: Vec<u8>,
    /// Whether RFC 3986 (`rawurlencode`) encoding was requested.
    raw: bool,
}

/// Appends one array level's `name=value` pairs to `out`; `prefix` is `None` at the top level.
fn eval_http_build_query_level(
    array: RuntimeCellHandle,
    prefix: Option<&[u8]>,
    query: &EvalQuery,
    out: &mut Vec<u8>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_iter_value(array, position);
        let value = match value {
            Ok(value) => value,
            Err(status) => {
                let _ = values.release(key);
                return Err(status);
            }
        };
        let appended =
            eval_http_build_query_pair(key, value, prefix, query, out, context, values);
        let released_value = values.release(value);
        let released_key = values.release(key);
        appended?;
        released_value?;
        released_key?;
    }
    Ok(())
}

/// Appends one key/value entry (recursing into arrays and objects) to `out`.
fn eval_http_build_query_pair(
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
    prefix: Option<&[u8]>,
    query: &EvalQuery,
    out: &mut Vec<u8>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let value_tag = values.type_tag(value)?;
    if matches!(value_tag, EVAL_TAG_NULL | EVAL_TAG_RESOURCE) {
        return Ok(());
    }
    let key_tag = values.type_tag(key)?;
    let key_bytes = values.string_bytes(key)?;
    let mut name = Vec::new();
    match prefix {
        None if key_tag == EVAL_TAG_INT => {
            name.extend_from_slice(&query.numeric_prefix);
            name.extend_from_slice(&key_bytes);
        }
        None => eval_http_build_query_encode(&key_bytes, query.raw, &mut name),
        Some(prefix) => {
            name.extend_from_slice(prefix);
            name.extend_from_slice(b"%5B");
            eval_http_build_query_encode(&key_bytes, query.raw, &mut name);
            name.extend_from_slice(b"%5D");
        }
    }
    let mut part = Vec::new();
    match value_tag {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            eval_http_build_query_level(value, Some(&name), query, &mut part, context, values)?;
        }
        EVAL_TAG_OBJECT => {
            let properties = eval_get_object_vars_result(&[value], context, values)?;
            let built = eval_http_build_query_level(
                properties,
                Some(&name),
                query,
                &mut part,
                context,
                values,
            );
            values.release(properties)?;
            built?;
        }
        EVAL_TAG_BOOL => {
            part.extend_from_slice(&name);
            part.extend_from_slice(if values.truthy(value)? { b"=1" } else { b"=0" });
        }
        _ => {
            let text = values.string_bytes(value)?;
            part.extend_from_slice(&name);
            part.push(b'=');
            eval_http_build_query_encode(&text, query.raw, &mut part);
        }
    }
    if part.is_empty() {
        return Ok(());
    }
    if !out.is_empty() {
        out.extend_from_slice(&query.separator);
    }
    out.extend_from_slice(&part);
    Ok(())
}

/// Percent-encodes `bytes` with `urlencode` (spaces as `+`) or `rawurlencode` rules.
fn eval_http_build_query_encode(bytes: &[u8], raw: bool, out: &mut Vec<u8>) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in bytes {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.')
            || (raw && byte == b'~');
        if keep {
            out.push(byte);
        } else if !raw && byte == b' ' {
            out.push(b'+');
        } else {
            out.push(b'%');
            out.push(HEX[(byte >> 4) as usize]);
            out.push(HEX[(byte & 0x0f) as usize]);
        }
    }
}
