//! Purpose:
//! `curl_setopt($ch, CURLOPT_POSTFIELDS, [...])`'s `multipart/form-data` walk for `eval()`
//! — the eval twin of `crate::curl_prelude::__elephc_curl_build_multipart`, driving the
//! same `elephc_curl_mime_*` ABI part for part.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl::handle`'s `eval_curl_setopt_apply`.
//!
//! Key details — every one of these is the AOT walker's own, reproduced rather than
//! reinvented (that function's header carries the measurements behind each):
//!
//! - AN EMPTY ARRAY IS AN EMPTY STRING BODY, NOT AN EMPTY MULTIPART. php-src
//!   special-cases it before building any mime structure, and it is observable on the wire:
//!   `CURLOPT_POSTFIELDS => []` sends `Content-Type: application/x-www-form-urlencoded`
//!   with an empty body — byte for byte what `CURLOPT_POSTFIELDS => ""` sends — while a
//!   built-but-empty `curl_mime` would send a `multipart/form-data` content type and a
//!   boundary-only body. Handled by the CALLER (`eval_curl_setopt_apply`), matching where
//!   the AOT prelude handles it.
//! - `CURLFile` -> a FILE part read from disk at transfer time: `FIELD_FILEDATA` = `$f->name`,
//!   `FIELD_TYPE` = `$f->mime` OR THE LITERAL `"application/octet-stream"` WHEN EMPTY —
//!   ALWAYS SET, NEVER SKIPPED, because libcurl SNIFFS an unset file-part type from the
//!   POSTED filename's extension (`Curl_mime_prepare_headers`) and php-src always passes an
//!   explicit value. `FIELD_FILENAME` = `$f->postname` unless empty, in which case it is
//!   `$f->name` VERBATIM — the full path as given, NOT its `basename()`. That looks like a
//!   bug and users have reported it as one; it is what a real `ext/curl` sends.
//! - `CURLStringFile` -> an IN-MEMORY file part: `FIELD_DATA` = `$f->data` (binary-safe),
//!   `FIELD_FILENAME` = `$f->postname` (always — the constructor requires it),
//!   `FIELD_TYPE` = `$f->mime` (always — the constructor defaults it to
//!   `"application/octet-stream"`).
//! - A NESTED ARRAY VALUE FLATTENS ONE LEVEL: one part per INNER element, every one named
//!   with the SAME outer key, the inner keys discarded — php-src's own repeated-field idiom,
//!   measured against a real `ext/curl` rather than assumed.
//! - ANY OTHER OBJECT IS REFUSED, LOUDLY, with a catchable `\TypeError`, rather than
//!   string-cast. php-src would `zval_get_tmp_string` it (posting a `Stringable`'s value and
//!   raising a catchable `\Error` otherwise), but elephc's own object-to-string cast for a
//!   class with no `__toString()` is an UNCATCHABLE process exit — relying on it would make
//!   a bad `CURLOPT_POSTFIELDS` value kill the process. Same divergence, same wording, as
//!   the AOT walker.
//! - EVERY FAILURE PATH CALLS `mime_abort()` BEFORE RETURNING OR THROWING, so a walk that
//!   dies partway never leaves the half-built structure dangling and never disturbs whatever
//!   mime is already ATTACHED from an earlier successful call on the same handle.

use crate::curl_ffi as ffi;

use super::*;

/// The class names this walker treats as file parts. Compared through
/// `RuntimeValueOps::object_is_a`, which honours PHP's own subclassing rules —
/// `CURLFile` is NOT `final` and userland subclasses of it are legal (verified against a
/// real PHP 8.4.20 `ext/curl`), so a plain class-name equality check would wrongly reject
/// one. `CURLStringFile` does NOT extend `CURLFile`, so the two are tested independently
/// and `CURLFile` is tested LAST for exactly that reason to stay order-independent.
const CURL_FILE_CLASS: &str = "CURLFile";
const CURL_STRING_FILE_CLASS: &str = "CURLStringFile";

/// Builds and attaches the `multipart/form-data` structure for one `CURLOPT_POSTFIELDS`
/// array, returning `curl_setopt()`'s own `bool`.
pub(in crate::interpreter) fn eval_curl_build_multipart(
    raw: i64,
    fields: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !ffi::mime_new(raw) {
        return values.bool_value(false);
    }
    let len = values.array_len(fields)?;
    for position in 0..len {
        let key = values.array_iter_key(fields, position)?;
        let value = values.array_get(fields, key)?;
        let name = values.cast_string(key)?;
        let name = values.string_bytes(name)?;
        if eval_curl_is_php_array(value, values)? {
            if !eval_curl_multipart_nested(raw, &name, value, context, values)? {
                return values.bool_value(false);
            }
            continue;
        }
        if !ffi::mime_add_part(raw) {
            ffi::mime_abort(raw);
            return values.bool_value(false);
        }
        if !ffi::mime_part_field(raw, ffi::MIME_FIELD_NAME, &name) {
            ffi::mime_abort(raw);
            return values.bool_value(false);
        }
        if values.type_tag(value)? == EVAL_TAG_OBJECT {
            if !eval_curl_multipart_object_part(raw, value, context, values)? {
                return values.bool_value(false);
            }
            continue;
        }
        let scalar = values.cast_string(value)?;
        let scalar = values.string_bytes(scalar)?;
        if !ffi::mime_part_field(raw, ffi::MIME_FIELD_DATA, &scalar) {
            ffi::mime_abort(raw);
            return values.bool_value(false);
        }
    }
    values.bool_value(ffi::mime_post(raw))
}

/// Emits one part per element of a nested array value, all sharing the outer key as their
/// field name. Returns `false` when the walk failed (already aborted).
fn eval_curl_multipart_nested(
    raw: i64,
    name: &[u8],
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let len = values.array_len(value)?;
    for position in 0..len {
        let key = values.array_iter_key(value, position)?;
        let inner = values.array_get(value, key)?;
        // DIVERGENCE FROM PHP, the AOT walker's own: going one level deeper stops php-src's
        // recursion and raises its ordinary `Warning: Array to string conversion`, posting
        // the literal string `"Array"`. Reproducing that warn-and-mangle shape was judged
        // not worth the complexity; an inner element that is itself an array or object gets
        // a clear, loud `\TypeError` instead of a silently mangled request.
        if eval_curl_is_php_array(inner, values)? || values.type_tag(inner)? == EVAL_TAG_OBJECT {
            ffi::mime_abort(raw);
            let _: RuntimeCellHandle = eval_throw_type_error(
                "curl_setopt(): CURLOPT_POSTFIELDS nested array value must contain only scalars",
                context,
                values,
            )?;
        }
        if !ffi::mime_add_part(raw) {
            ffi::mime_abort(raw);
            return Ok(false);
        }
        if !ffi::mime_part_field(raw, ffi::MIME_FIELD_NAME, name) {
            ffi::mime_abort(raw);
            return Ok(false);
        }
        let inner = values.cast_string(inner)?;
        let inner = values.string_bytes(inner)?;
        if !ffi::mime_part_field(raw, ffi::MIME_FIELD_DATA, &inner) {
            ffi::mime_abort(raw);
            return Ok(false);
        }
    }
    Ok(true)
}

/// Fills the current part from a `CURLFile`/`CURLStringFile`, or throws for any other
/// object. The part's NAME field has already been written by the caller.
fn eval_curl_multipart_object_part(
    raw: i64,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.object_is_a(value, CURL_STRING_FILE_CLASS, false)? {
        let data = eval_curl_multipart_property(value, "data", values)?;
        if !ffi::mime_part_field(raw, ffi::MIME_FIELD_DATA, &data) {
            ffi::mime_abort(raw);
            return Ok(false);
        }
        let postname = eval_curl_multipart_property(value, "postname", values)?;
        if !ffi::mime_part_field(raw, ffi::MIME_FIELD_FILENAME, &postname) {
            ffi::mime_abort(raw);
            return Ok(false);
        }
        let mime = eval_curl_multipart_property(value, "mime", values)?;
        if !ffi::mime_part_field(raw, ffi::MIME_FIELD_TYPE, &mime) {
            ffi::mime_abort(raw);
            return Ok(false);
        }
        return Ok(true);
    }
    if values.object_is_a(value, CURL_FILE_CLASS, false)? {
        let path = eval_curl_multipart_property(value, "name", values)?;
        if !ffi::mime_part_field(raw, ffi::MIME_FIELD_FILEDATA, &path) {
            ffi::mime_abort(raw);
            return Ok(false);
        }
        // ALWAYS SET, NEVER SKIPPED — see this module's header: skipping it is not "no
        // type", it is "whatever libcurl sniffs from the posted filename's extension".
        let mime = eval_curl_multipart_property(value, "mime", values)?;
        let mime: &[u8] = if mime.is_empty() {
            b"application/octet-stream"
        } else {
            &mime
        };
        if !ffi::mime_part_field(raw, ffi::MIME_FIELD_TYPE, mime) {
            ffi::mime_abort(raw);
            return Ok(false);
        }
        // NO `basename()` HERE — the full path is the measured, correct fallback.
        let postname = eval_curl_multipart_property(value, "postname", values)?;
        let filename: &[u8] = if postname.is_empty() { &path } else { &postname };
        if !ffi::mime_part_field(raw, ffi::MIME_FIELD_FILENAME, filename) {
            ffi::mime_abort(raw);
            return Ok(false);
        }
        return Ok(true);
    }
    ffi::mime_abort(raw);
    let class_name = eval_curl_given_type_name(value, context, values)?;
    let _: RuntimeCellHandle = eval_throw_type_error(
        &format!(
            "curl_setopt(): CURLOPT_POSTFIELDS array value must be of type \
             string|int|float|bool|CURLFile|CURLStringFile, {class_name} given"
        ),
        context,
        values,
    )?;
    Ok(false)
}

/// Reads one `CURLFile`/`CURLStringFile` property as raw bytes. The properties are declared
/// `public string`, so a non-string here is not reachable from a well-formed object; the
/// cast is kept anyway rather than assumed away, because eval can be handed a subclass that
/// re-declared them.
fn eval_curl_multipart_property(
    object: RuntimeCellHandle,
    property: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let value = values.property_get(object, property)?;
    let value = values.cast_string(value)?;
    values.string_bytes(value)
}
