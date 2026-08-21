//! Purpose:
//! Eval home for `curl_getinfo(CurlHandle $handle, ?int $option = null): mixed`.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl` dispatch.
//!
//! Key details:
//! - DISPATCHES ON THE OPTION'S TYPE MASK, mirroring `crate::curl_prelude::curl_getinfo`
//!   exactly (see that function's own extensive comment for the mask values and why three
//!   options are special-cased before the mask). `CURLINFO_HEADER_OUT` always answers
//!   `false` here too, for the identical reason the AOT wrapper documents (the header
//!   capture needs the `CURLOPT_DEBUGFUNCTION` plumbing this family does not implement).

use crate::curl_ffi as ffi;

eval_builtin! {
    contract: "curl_getinfo",
    area: Curl,
    direct: Curl,
    values: Curl,
}

use super::*;

/// `CURLINFO_PRIVATE`: answered from the eval-side mirror, never the bridge.
const CURLINFO_PRIVATE: i64 = 1_048_597;
/// `CURLINFO_HEADER_OUT`: always `false` here (module doc).
const CURLINFO_HEADER_OUT: i64 = 2;
/// `CURLINFO_CERTINFO`: SLIST-tagged but really a `struct curl_certinfo *`.
const CURLINFO_CERTINFO: i64 = 4_194_338;
/// `CURLINFO_TYPEMASK`.
const CURLINFO_TYPEMASK: i64 = 15_728_640;
const CURLINFO_LONG_MASK: i64 = 2_097_152;
const CURLINFO_OFF_T_MASK: i64 = 6_291_456;
const CURLINFO_DOUBLE_MASK: i64 = 3_145_728;
const CURLINFO_STRING_MASK: i64 = 1_048_576;
const CURLINFO_SLIST_MASK: i64 = 4_194_304;

/// Evaluates `curl_getinfo($handle, $option)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_curl_getinfo(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (handle, option) = match args {
        [handle] => (eval_expr(handle, context, scope, values)?, None),
        [handle, option] => (
            eval_expr(handle, context, scope, values)?,
            Some(eval_expr(option, context, scope, values)?),
        ),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_curl_getinfo_result(handle, option, context, values)
}

/// Dispatches evaluated `curl_getinfo()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_curl_getinfo_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (handle, option) = match evaluated_args {
        [handle] => (*handle, None),
        [handle, option] => (*handle, Some(*option)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_curl_getinfo_result(handle, option, context, values)
}

fn eval_curl_getinfo_result(
    handle: RuntimeCellHandle,
    option: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (table_id, raw) = eval_curl_easy_handle("curl_getinfo", handle, context, values)?;

    let option = match option {
        None => None,
        Some(option) if values.type_tag(option)? == EVAL_TAG_NULL => None,
        Some(option) => Some(eval_int_value(option, values)?),
    };

    let Some(option) = option else {
        // The no-`$option` associative array, built the same way `curl_version()` is:
        // decode the bridge's JSON blob through the ordinary `json_decode()` builtin.
        let json = ffi::easy_str_op(raw, ffi::STR_OP_INFO_ALL, &[], 0);
        let Some(json) = json else {
            return values.bool_value(false);
        };
        let json_cell = values.string_bytes_value(&json)?;
        let associative = values.bool_value(true)?;
        let decoded = crate::interpreter::builtins::json::eval_json_decode_values_result(
            &[json_cell, associative],
            context,
            values,
        )?;
        return if values.is_array_like(decoded)? {
            Ok(decoded)
        } else {
            values.bool_value(false)
        };
    };

    if option == CURLINFO_PRIVATE {
        return match context.stream_resources().curl_easy_private(table_id) {
            Some(stored) => values.retain(stored),
            None => values.bool_value(false),
        };
    }
    if option == CURLINFO_HEADER_OUT {
        return values.bool_value(false);
    }
    if option == CURLINFO_CERTINFO {
        let json = ffi::easy_str_op(raw, ffi::STR_OP_INFO_CERTINFO, &[], option);
        let Some(json) = json else {
            return values.bool_value(false);
        };
        let json_cell = values.string_bytes_value(&json)?;
        let associative = values.bool_value(true)?;
        let decoded = crate::interpreter::builtins::json::eval_json_decode_values_result(
            &[json_cell, associative],
            context,
            values,
        )?;
        return if values.is_array_like(decoded)? {
            Ok(decoded)
        } else {
            values.bool_value(false)
        };
    }

    let Ok(info) = i32::try_from(option) else {
        return values.bool_value(false);
    };
    let mask = option & CURLINFO_TYPEMASK;
    if mask == CURLINFO_LONG_MASK || mask == CURLINFO_OFF_T_MASK {
        return match ffi::easy_getinfo_long(raw, info) {
            Some(value) => values.int(value),
            None => values.bool_value(false),
        };
    }
    if mask == CURLINFO_DOUBLE_MASK {
        return match ffi::easy_getinfo_double(raw, info) {
            Some(value) => values.float(value),
            None => values.bool_value(false),
        };
    }
    if mask == CURLINFO_STRING_MASK {
        return match ffi::easy_str_op(raw, ffi::STR_OP_INFO_STRING, &[], option) {
            Some(bytes) => values.string_bytes_value(&bytes),
            None => values.bool_value(false),
        };
    }
    if mask == CURLINFO_SLIST_MASK {
        let Some(blob) = ffi::easy_str_op(raw, ffi::STR_OP_INFO_SLIST, &[], option) else {
            return values.bool_value(false);
        };
        return eval_curl_slist_blob_to_array(&blob, values);
    }
    values.bool_value(false)
}

/// Converts a NUL-FRAMED item blob (`item . "\0"` per entry — the same framing
/// `curl_setopt()`'s string-list options send) into a plain PHP list of strings, mirroring
/// `crate::curl_prelude::curl_getinfo`'s `explode("\0", substr($text, 0, strlen($text) -
/// 1))`.
fn eval_curl_slist_blob_to_array(
    blob: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if blob.is_empty() {
        return values.string_array_new(0);
    }
    // Every item (including the last) is written with its own trailing NUL, so dropping
    // the final byte before splitting on NUL as a SEPARATOR yields exactly one fragment
    // per item, with no trailing empty fragment.
    let trimmed = &blob[..blob.len() - 1];
    let items: Vec<&[u8]> = trimmed.split(|&byte| byte == 0).collect();
    let mut array = values.string_array_new(items.len())?;
    for item in items {
        // `RuntimeValueOps::string_array_push` takes `&str`, not raw bytes: every
        // `CURLINFO_SLIST` field this build exposes (`CURLINFO_COOKIELIST` etc.) is
        // ASCII/UTF-8 in practice, so lossy conversion is a no-op for real values; a
        // theoretical non-UTF-8 entry loses fidelity here the same way any other
        // `&str`-typed eval array builtin would.
        let item = String::from_utf8_lossy(item);
        array = values.string_array_push(array, &item)?;
    }
    Ok(array)
}
