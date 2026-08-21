//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `iconv_mime_encode()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - `$options` accepts `scheme`, `input-charset`, `output-charset`, `line-length`, and
//!   `line-break-chars`; every other key is ignored like php-src does.
//! - php-src only honors a string `scheme` or charset, but coerces `line-length`.

eval_builtin! {
    contract: "iconv_mime_encode",
    area: String,
    direct: Iconv,
    values: Iconv,
}

use super::super::super::*;
use super::iconv::{eval_iconv_bytes};

use elephc_iconv::{MimeEncodeOptions, Scheme};

/// Applies PHP `iconv_mime_encode(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_iconv_mime_encode_result(
    field_name: RuntimeCellHandle,
    field_value: RuntimeCellHandle,
    options: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let field_name = values.string_bytes(field_name)?;
    let field_value = values.string_bytes(field_value)?;
    let options = eval_iconv_mime_options(options, values)?;
    let encoded = elephc_iconv::mime_encode(&field_name, &field_value, &options);
    eval_iconv_bytes("iconv_mime_encode", encoded, values)
}

/// Reads the recognized option keys out of the caller's array.
///
/// php-src takes `input-charset` as the default for `output-charset`, so the two are
/// resolved in that order.
fn eval_iconv_mime_options(
    options: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<MimeEncodeOptions, EvalStatus> {
    let mut resolved = MimeEncodeOptions::default();
    let Some(options) = options else {
        return Ok(resolved);
    };
    if values.is_null(options)? {
        return Ok(resolved);
    }
    if let Some(scheme) = eval_iconv_option_string(options, "scheme", values)? {
        resolved.scheme = Scheme::parse(&scheme);
    }
    if let Some(charset) = eval_iconv_option_string(options, "input-charset", values)? {
        resolved.input_charset = charset.clone();
        resolved.output_charset = charset;
    }
    if let Some(charset) = eval_iconv_option_string(options, "output-charset", values)? {
        resolved.output_charset = charset;
    }
    if let Some(length) = eval_iconv_option_value(options, "line-length", values)? {
        resolved.line_length = eval_int_value(length, values)?;
    }
    if let Some(breaks) = eval_iconv_option_string(options, "line-break-chars", values)? {
        resolved.line_break = breaks;
    }
    Ok(resolved)
}

/// Reads one option, returning `None` when the key is absent or holds an empty string.
fn eval_iconv_option_string(
    options: RuntimeCellHandle,
    key: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Vec<u8>>, EvalStatus> {
    let Some(value) = eval_iconv_option_value(options, key, values)? else {
        return Ok(None);
    };
    if values.type_tag(value)? != EVAL_TAG_STRING {
        return Ok(None);
    }
    let bytes = values.string_bytes(value)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(bytes))
}

/// Reads one raw option cell, returning `None` when the key is absent.
fn eval_iconv_option_value(
    options: RuntimeCellHandle,
    key: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let key = values.string(key)?;
    let exists = values.array_key_exists(key, options)?;
    if !values.truthy(exists)? {
        return Ok(None);
    }
    Ok(Some(values.array_get(options, key)?))
}
