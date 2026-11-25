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
use crate::runtime_hooks::owner_frames::{StagedRuntimeOwners, with_runtime_owners};

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
    if let Some(length) = eval_iconv_option(options, "line-length", values, |value, values| {
        eval_int_value(value, values).map(Some)
    })? {
        resolved.line_length = length;
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
    eval_iconv_option(options, key, values, |value, values| {
        if values.type_tag(value)? != EVAL_TAG_STRING {
            return Ok(None);
        }
        let bytes = values.string_bytes(value)?;
        Ok((!bytes.is_empty()).then_some(bytes))
    })
}

/// Reads one option under a temporary owner frame and retires every boxed lookup cell.
fn eval_iconv_option<R, V: RuntimeValueOps>(
    options: RuntimeCellHandle,
    key: &str,
    values: &mut V,
    read: impl FnOnce(RuntimeCellHandle, &mut V) -> Result<Option<R>, EvalStatus>,
) -> Result<Option<R>, EvalStatus> {
    let owners = StagedRuntimeOwners::default();
    with_runtime_owners(&owners, || {
        let result = (|| {
            let key = owners.stage(values.string(key)?);
            let exists = owners.stage(values.array_key_exists(key.borrowed(), options)?);
            if !values.truthy(exists.borrowed())? {
                return Ok(None);
            }
            let value = owners.stage(values.array_get(options, key.borrowed())?);
            read(value.borrowed(), values)
        })();
        owners.complete(result, |cell| values.release(cell))
    })
}
