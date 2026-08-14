//! Purpose:
//! Shared curl easy-handle resolution and `curl_setopt()`'s option-KIND dispatch, reused
//! by every home file in this family (mirrors `crate::curl_prelude::curl_setopt`'s body
//! one-for-one, minus the callback/share arms this module's doc defers).
//!
//! Called from:
//! - Every `curl_*` home file in this directory.

use crate::curl_ffi as ffi;

use super::*;

/// Resolves a `curl_init()`-produced handle cell to its bridge raw id, validating that
/// the eval table key actually names a LIVE curl easy handle (not a foreign resource cell,
/// and not one this same `ElephcEvalContext` never created).
pub(in crate::interpreter) fn eval_curl_easy_raw(
    handle: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let id = eval_resource_payload(handle, values)?;
    context
        .stream_resources()
        .curl_easy_raw(id)
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Resolves a handle cell to its EVAL TABLE KEY (not the bridge's raw id) for callers that
/// need to read/write the PHP-layer mirror fields (`EvalStreamResources::curl_easy_*`).
pub(in crate::interpreter) fn eval_curl_easy_table_id(
    handle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    eval_resource_payload(handle, values)
}

/// `curl_setopt()`'s message for KIND 6 (a real option this build cannot carry) and KIND 7/
/// 8 (share/callback — accepted PHP API this eval interpreter specifically does not wire,
/// per this family's module doc), formatted exactly like the AOT prelude's own
/// `__elephc_curl_setopt_unsupported_warning` (`src/codegen_support/runtime/curl/
/// warn_option.rs`'s `CURL_SETOPT_UNSUPPORTED_PREFIX`/`_SUFFIX`), so a script observing
/// the warning text sees the identical wording whether it runs compiled or through
/// `eval()`.
fn warn_unsupported_option(
    option: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    values.warning(&format!(
        "curl_setopt(): Option {option} is not supported by this build"
    ))
}

/// Applies one `curl_setopt($handle, $option, $value)` call, given the handle's bridge raw
/// id, EVAL TABLE KEY (for the PHP-layer mirror fields), and already-evaluated option/value
/// cells. Returns the same `bool` `curl_setopt()` itself returns.
///
/// Mirrors `crate::curl_prelude::curl_setopt`'s body kind-for-kind (see that function's own
/// extensive comments for the libcurl-side rationale of each branch), MINUS the KIND 3
/// (`CURLOPT_POSTFIELDS` array/`multipart`) special case, KIND 7 (`CURLOPT_SHARE`), and
/// KIND 8 (callbacks) — all three fall into the honest "not supported by this build"
/// warning path instead, per this family's module doc.
pub(in crate::interpreter) fn eval_curl_setopt_apply(
    raw: i64,
    table_id: i64,
    option: i64,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Ok(opt) = i32::try_from(option) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let kind = ffi::option_kind(option);
    if kind == ffi::KIND_INVALID {
        // php-src's own `ValueError` for an option number it does not recognize at all
        // (`crate::curl_prelude::curl_setopt`'s header). No catchable-exception path
        // exists from inside this interpreter's internals, so this is a hard fault —
        // the same tradeoff every other "should not realistically happen" guard in this
        // crate already makes (e.g. `hash_final` on an already-finalized context).
        return Err(EvalStatus::RuntimeFatal);
    }
    if kind == ffi::KIND_SLIST {
        if !values.is_array_like(value)? {
            return values.bool_value(false);
        }
        let mut blob = Vec::new();
        let len = values.array_len(value)?;
        for position in 0..len {
            let key = values.array_iter_key(value, position)?;
            let item = values.array_get(value, key)?;
            let item = values.cast_string(item)?;
            blob.extend_from_slice(&values.string_bytes(item)?);
            blob.push(0);
        }
        return values.bool_value(ffi::easy_setopt_slist(raw, opt, &blob));
    }
    if kind == ffi::KIND_PHP_LAYER {
        // CURLOPT_RETURNTRANSFER (19913): mirrored onto the eval-side mirror fields
        // because `curl_exec()`'s return shape depends on it, and forwarded to the
        // bridge because the write callback's capture-or-stdout decision lives there.
        if option == 19913 {
            let truthy = values.truthy(value)?;
            context
                .stream_resources_mut()
                .set_curl_easy_write_mode(table_id, truthy, false);
            return values.bool_value(ffi::easy_setopt_long(raw, opt, i64::from(truthy)));
        }
        // CURLOPT_PRIVATE (10103): retained and stored, read back by
        // `curl_getinfo(..., CURLINFO_PRIVATE)`. `set_curl_easy_private` retains its own
        // independent reference — see that method's doc for why storing the caller's bare
        // (unretained) `value` cell here would be a use-after-free as soon as the caller's
        // own variable is unset or reassigned.
        if option == 10103 {
            let stored = context
                .stream_resources_mut()
                .set_curl_easy_private(table_id, value, values)?;
            return values.bool_value(stored);
        }
        // CURLOPT_SAFE_UPLOAD (-1): always on, matching php-src's own rejection of a
        // falsy value.
        if option == -1 {
            if !values.truthy(value)? {
                return Err(EvalStatus::RuntimeFatal);
            }
            return values.bool_value(true);
        }
        // CURLOPT_BINARYTRANSFER (19914): documented no-op in modern PHP.
        return values.bool_value(true);
    }
    // KIND_STREAM IS IN THIS LIST FOR A REASON THE OTHERS ARE NOT: without it, the four
    // PHP-stream options would fall PAST the warning and into the scalar-type guard below,
    // where a stream resource is none of int/string/float/bool and therefore a HARD FATAL.
    // They used to be `KIND_UNSUPPORTED`, so leaving them out here would have turned a
    // `false` + warning into an uncatchable fault the moment the AOT side implemented them.
    if kind == ffi::KIND_SHARE
        || kind == ffi::KIND_CALLBACK
        || kind == ffi::KIND_STREAM
        || kind == ffi::KIND_UNSUPPORTED
    {
        warn_unsupported_option(option, values)?;
        return values.bool_value(false);
    }
    // CURLOPT_POSTFIELDS (10015) with an ARRAY value posts real `multipart/form-data` in
    // the AOT build (`crate::curl_prelude::__elephc_curl_build_multipart`, Task 11), which
    // needs `CURLFile`/`CURLStringFile` — deferred here (this family's module doc). The
    // plain STRING form (the common urlencoded-body case) still works below through the
    // ordinary KIND_STRING path.
    if option == 10015 && values.is_array_like(value)? {
        warn_unsupported_option(option, values)?;
        return values.bool_value(false);
    }
    // php-src rejects a non-scalar `$value` with a catchable `\TypeError` here
    // (`crate::curl_prelude::curl_setopt`'s own guard). This interpreter has no
    // catchable-exception path from internals, so the fault is a hard one instead — see
    // `KIND_INVALID`'s branch above for the same tradeoff.
    let tag = values.type_tag(value)?;
    if !matches!(
        tag,
        EVAL_TAG_INT | EVAL_TAG_STRING | EVAL_TAG_FLOAT | EVAL_TAG_BOOL
    ) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if kind == ffi::KIND_STRING {
        let value = values.cast_string(value)?;
        let bytes = values.string_bytes(value)?;
        return values.bool_value(ffi::easy_setopt_str(raw, opt, &bytes));
    }
    if kind == ffi::KIND_LONG || kind == ffi::KIND_OFF_T {
        let value = eval_int_value(value, values)?;
        return values.bool_value(ffi::easy_setopt_long(raw, opt, value));
    }
    warn_unsupported_option(option, values)?;
    values.bool_value(false)
}
