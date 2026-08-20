//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `iconv_set_encoding()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - php-src stores the new charset without validating it, so only an unrecognized
//!   `$type` makes the call report `false`.

eval_builtin! {
    contract: "iconv_set_encoding",
    area: String,
    direct: Iconv,
    values: Iconv,
}

use super::super::super::*;

use elephc_iconv::EncodingKind;

/// Applies PHP `iconv_set_encoding(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_iconv_set_encoding_result(
    requested: RuntimeCellHandle,
    encoding: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let requested = values.string_bytes(requested)?;
    let encoding = values.string_bytes(encoding)?;
    let Some(kind) = EncodingKind::parse(&requested) else {
        return values.bool_value(false);
    };
    let applied = elephc_iconv::set(kind, &encoding);
    values.bool_value(applied)
}
