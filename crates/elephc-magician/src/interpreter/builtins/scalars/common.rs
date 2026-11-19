//! Purpose:
//! Common scalar conversion and checksum helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::scalars` re-exports.
//!
//! Key details:
//! - Runtime cells remain opaque and all PHP coercions flow through `RuntimeValueOps`.
//! - Weak float-to-int coercion reports PHP's precision-loss deprecation before casting.

use super::super::super::*;
use crate::stream_resources::{EVAL_DEFAULT_CONTEXT_PAYLOAD, EVAL_RESOURCE_PAYLOAD_BASE};

/// Returns the standard zlib/PHP CRC-32 checksum for a byte slice.
pub(in crate::interpreter) fn eval_crc32_bytes(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// Returns the eval-local native payload carried by a runtime resource cell.
///
/// Reads the tag-9 payload word straight out of the cell instead of casting the
/// cell to PHP int and undoing a `+ 1`. Those are two different numbers: `(int)`
/// on a resource yields the PHP RESOURCE ID, which the runtime mints from its own
/// counter (`runtime::resource_ids`) precisely so that a displayed id never
/// depends on a native payload. The id therefore cannot be inverted back into the
/// zero-based key of `EvalStreamResources`, and any attempt to do so silently
/// resolves the wrong stream. The raw word is the only faithful source, and it is
/// the same accessor the native-argument and by-reference writeback paths already
/// use for tag-9 slots.
pub(in crate::interpreter) fn eval_resource_payload(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    i64::try_from(values.raw_value_word(value)?).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Returns whether a runtime resource cell refers to an ALREADY-CLOSED handle.
///
/// PHP 8.5.6 renames a closed resource to `Unknown` in both `var_dump()` and
/// `get_resource_type()`, so every eval display path needs this answer. The two resource
/// origins record closure in two different, deliberately unmerged ways:
///
/// - A HOST resource closed by the compiled program carries the `-id` sentinel
///   `fclose`/`pclose`/`closedir` stamp into its Mixed box (see
///   `elephc::codegen::lower_inst::builtins::io`), so a negative payload is closed.
/// - An EVAL-CREATED resource carries no sentinel, because its payload IS the key of
///   `EvalStreamResources`; negating it would break every builtin that later resolves the
///   handle. Its close state lives in those tables, so `EvalStreamResources::is_live`
///   answers for it.
///
/// The payload is read as `as i64`, NOT through `i64::try_from`. `raw_value_word` returns
/// `u64` and the sentinel for id 5 is `0xFFFF_FFFF_FFFF_FFFB`, which `i64::try_from`
/// rejects — the exact trap `eval_resource_payload` above falls into for a closed handle.
pub(in crate::interpreter) fn eval_resource_is_closed(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let payload = values.raw_value_word(value)? as i64;
    if payload < 0 {
        return Ok(true);
    }
    if payload >= EVAL_RESOURCE_PAYLOAD_BASE {
        return Ok(!context.stream_resources().is_live(payload));
    }
    Ok(false)
}

/// Returns the PHP resource type name a display path should print for one resource cell.
///
/// `"stream"` while the handle is open — the single type elephc models today — and
/// `"Unknown"` once it has been closed, which is what PHP 8.5.6 reports for a closed
/// `fopen` stream, a closed `popen` pipe and a closed `opendir` handle alike.
pub(in crate::interpreter) fn eval_resource_type_name(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<&'static str, EvalStatus> {
    let payload = values.raw_value_word(value)? as i64;
    if payload == EVAL_DEFAULT_CONTEXT_PAYLOAD {
        return Ok("stream-context");
    }
    if eval_resource_is_closed(value, context, values)? {
        return Ok("Unknown");
    }
    Ok(context
        .stream_resources()
        .resource_type(payload)
        .unwrap_or("stream"))
}

/// Applies PHP's coercive-mode `int` parameter rules to one internal-function argument.
///
/// Ints and bools pass, integral-range floats and numeric strings (surrounding whitespace
/// allowed) truncate with PHP's precision deprecations, and null becomes 0 with the
/// internal-function null deprecation. Returns `None` for every value PHP refuses (arrays,
/// objects, resources, closures, non-numeric or leading-numeric strings, NaN and out-of-range
/// floats) so the caller can throw its `TypeError` or emit its warning before any side effect.
pub(in crate::interpreter) fn eval_weak_int_param(
    value: RuntimeCellHandle,
    name: &str,
    number: usize,
    parameter: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<i64>, EvalStatus> {
    let tag = values.type_tag(value)?;
    if tag == EVAL_TAG_STRING {
        let bytes = values.string_bytes(value)?;
        if let Ok(source) = std::str::from_utf8(&bytes) {
            // PHP numeric strings also accept vertical tab, unlike Rust's ASCII whitespace set.
            let numeric = source.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}'));
            if let Ok(integer) = numeric.parse::<i64>() {
                return Ok(Some(integer));
            }
            if let Ok(float) = numeric.parse::<f64>() {
                if eval_float_fits_php_int(float) {
                    if float.trunc() != float {
                        values.warning(&format!(
                            "Deprecated: Implicit conversion from float-string \"{source}\" to int loses precision\n"
                        ))?;
                    }
                    return Ok(Some(float.trunc() as i64));
                }
            }
        }
    } else if tag == EVAL_TAG_FLOAT {
        let float = f64::from_bits(values.raw_value_word(value)?);
        if eval_float_fits_php_int(float) {
            return eval_int_value(value, values).map(Some);
        }
    } else if tag == EVAL_TAG_INT || tag == EVAL_TAG_BOOL {
        return eval_int_value(value, values).map(Some);
    } else if tag == EVAL_TAG_NULL {
        values.warning(&format!(
            "Deprecated: {name}(): Passing null to parameter #{number} (${parameter}) of type int is deprecated\n"
        ))?;
        return Ok(Some(0));
    }
    Ok(None)
}

/// Applies PHP's `int` parameter rules for the calling code's mode.
///
/// Under `declare(strict_types=1)` only an int passes, so strings, floats, bools and null are
/// all refused; otherwise `eval_weak_int_param()` applies the coercive rules. Returns `None`
/// for a refused value so the caller can throw or warn before any side effect.
pub(in crate::interpreter) fn eval_mode_int_param(
    value: RuntimeCellHandle,
    name: &str,
    number: usize,
    parameter: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<i64>, EvalStatus> {
    if context.strict_types() {
        if values.type_tag(value)? == EVAL_TAG_INT {
            return eval_int_value(value, values).map(Some);
        }
        return Ok(None);
    }
    eval_weak_int_param(value, name, number, parameter, values)
}

/// Applies `eval_mode_int_param()` and throws PHP's `TypeError` for a refused value.
pub(in crate::interpreter) fn eval_int_param(
    value: RuntimeCellHandle,
    name: &str,
    number: usize,
    parameter: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if let Some(integer) = eval_mode_int_param(value, name, number, parameter, context, values)? {
        return Ok(integer);
    }
    let given = eval_given_type_name(value, context, values)?;
    eval_throw_type_error(
        &format!("{name}(): Argument #{number} (${parameter}) must be of type int, {given} given"),
        context,
        values,
    )
}

/// Mirrors Zend's inclusive lower bound and exclusive rounded upper bound for doubles.
fn eval_float_fits_php_int(value: f64) -> bool {
    value.is_finite() && value >= i64::MIN as f64 && value < -(i64::MIN as f64)
}

/// Casts one eval value to PHP int and returns the scalar payload.
pub(in crate::interpreter) fn eval_int_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(value)? == EVAL_TAG_FLOAT {
        let display = values.string_bytes(value)?;
        let float = std::str::from_utf8(&display)
            .map_err(|_| EvalStatus::RuntimeFatal)?
            .parse::<f64>()
            .map_err(|_| EvalStatus::RuntimeFatal)?;
        if float.is_finite() && float.trunc() != float {
            values.warning(&format!(
                "Deprecated: Implicit conversion from float {} to int loses precision\n",
                String::from_utf8_lossy(&display)
            ))?;
        }
    }
    let integer = values.cast_int(value)?;
    let bytes = values.string_bytes(integer);
    let released = values.release(integer);
    let bytes = bytes?;
    released?;
    std::str::from_utf8(&bytes)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .parse::<i64>()
        .map_err(|_| EvalStatus::RuntimeFatal)
}
