//! Purpose:
//! Declarative eval registry entry for `ctype_alnum`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing ASCII ctype hook.

eval_builtin! {
    contract: "ctype_alnum",
    area: String,
    direct: Ctype,
    values: Ctype,
}

use super::super::super::*;

/// Evaluates PHP `ctype_alnum(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_ctype_alnum(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_builtin_ctype_named("ctype_alnum", args, context, scope, values)
}

/// Returns the PHP boolean result for `ctype_alnum(...)` from one evaluated value.
pub(in crate::interpreter) fn eval_ctype_alnum_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_ctype_named_result("ctype_alnum", value, values)
}

/// Evaluates a named PHP `ctype_*` predicate over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_ctype_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_ctype_named_result(name, value, values)
}

/// Returns the PHP boolean result for one named ASCII `ctype_*` byte-string check.
///
/// php-src takes `mixed`: a string is checked byte by byte, an int in -128..=255 is ONE character
/// code (negative values wrap by 256, so `-1` is byte 255), any other int is checked as its decimal
/// string, and every other type answers false. Coercing everything to a string made
/// `ctype_digit(7)` true, where php checks the bell character and answers false.
pub(in crate::interpreter) fn eval_ctype_named_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    const INT_TAG: u64 = 0;
    const STRING_TAG: u64 = 1;
    let bytes = match values.type_tag(value)? {
        STRING_TAG => values.string_bytes(value)?,
        INT_TAG => {
            let number = values.raw_value_word(value)? as i64;
            if (-128..=255).contains(&number) {
                vec![(number & 0xff) as u8]
            } else {
                number.to_string().into_bytes()
            }
        }
        _ => return values.bool_value(false),
    };
    let mut matches = !bytes.is_empty();
    for byte in bytes {
        if !eval_ctype_byte_matches(name, byte)? {
            matches = false;
            break;
        }
    }
    values.bool_value(matches)
}

/// Checks one byte against the selected PHP ASCII character class.
pub(in crate::interpreter) fn eval_ctype_byte_matches(
    name: &str,
    byte: u8,
) -> Result<bool, EvalStatus> {
    match name {
        "ctype_alpha" => Ok(byte.is_ascii_alphabetic()),
        "ctype_digit" => Ok(byte.is_ascii_digit()),
        "ctype_alnum" => Ok(byte.is_ascii_alphanumeric()),
        "ctype_space" => Ok(matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
