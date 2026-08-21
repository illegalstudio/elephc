//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `iconv_get_encoding()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - `$type` defaults to `all`, which reports the whole trio as an associative array.
//! - An unrecognized `$type` answers `false` without any diagnostic.

eval_builtin! {
    contract: "iconv_get_encoding",
    area: String,
    direct: Iconv,
    values: Iconv,
}

use super::super::super::*;

use elephc_iconv::EncodingKind;

/// Applies PHP `iconv_get_encoding(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_iconv_get_encoding_result(
    requested: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let requested = match requested {
        Some(requested) if !values.is_null(requested)? => values.string_bytes(requested)?,
        _ => b"all".to_vec(),
    };
    if requested.eq_ignore_ascii_case(b"all") {
        let mut result = values.assoc_new(3)?;
        for kind in EncodingKind::all() {
            let key = values.string(kind.key())?;
            let value = values.string(&elephc_iconv::get(kind))?;
            result = values.array_set(result, key, value)?;
        }
        return Ok(result);
    }
    match EncodingKind::parse(&requested) {
        Some(kind) => values.string(&elephc_iconv::get(kind)),
        None => values.bool_value(false),
    }
}
