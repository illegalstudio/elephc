//! Purpose:
//! Common scalar conversion and checksum helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::scalars` re-exports.
//!
//! Key details:
//! - Runtime cells remain opaque and all PHP coercions flow through `RuntimeValueOps`.

use super::super::super::*;
use crate::stream_resources::EVAL_RESOURCE_PAYLOAD_BASE;

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
/// - A HOST resource records closure in the compiled program's RESOURCE REGISTRY, which
///   only the runtime can read, so `RuntimeValueOps::resource_is_closed` asks it. Nothing
///   about the payload word answers this: since the generation-safe registry migration an
///   open host payload is an opaque handle, and `fclose` publishes the closed state on the
///   registry slot. Reading the word alone reported `stream` for every closed host handle
///   — `fclose($r); eval('var_dump($r);')` printed `resource(5) of type (stream)` while
///   the same program's NATIVE `var_dump($r)`, which already consults the registry
///   through `__rt_resource_type_name`, printed `(Unknown)`.
/// - An EVAL-CREATED resource is not in that registry at all: its payload IS the key of
///   `EvalStreamResources` (it carries `EVAL_RESOURCE_PAYLOAD_BASE`), and negating it
///   would break every builtin that later resolves the handle. Its close state lives in
///   those tables, so `EvalStreamResources::is_live` answers for it. Handing such a
///   payload to the registry would find no slot and report it open forever.
/// - The legacy `-id` sentinel arm stays first. `apply_resource_release_sentinel`
///   (`elephc::codegen::lower_inst::builtins::io`) still stamps a negative word into a
///   Mixed-typed box on close, and the runtime's own type-name helper keeps the same
///   short-circuit.
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
    values.resource_is_closed(payload as u64)
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
    if eval_resource_is_closed(value, context, values)? {
        Ok("Unknown")
    } else {
        Ok("stream")
    }
}

/// Casts one eval value to PHP int and returns the scalar payload.
pub(in crate::interpreter) fn eval_int_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let value = values.cast_int(value)?;
    let bytes = values.string_bytes(value)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .parse::<i64>()
        .map_err(|_| EvalStatus::RuntimeFatal)
}
