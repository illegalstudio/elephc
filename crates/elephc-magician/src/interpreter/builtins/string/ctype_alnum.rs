//! Purpose:
//! Declarative eval registry entry for `ctype_alnum`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing ASCII ctype hook.
//! - This file also hosts the hook every `ctype_*` binding shares: strings are classified in
//!   the C locale, other values raise php-src's deprecation and follow its int rules.

eval_builtin! {
    contract: "ctype_alnum",
    area: String,
    direct: Ctype,
    values: Ctype,
}

use super::super::super::*;

/// Evaluates PHP `ctype_alnum(...)` over one eval argument expression.
pub(in crate::interpreter) fn eval_builtin_ctype_alnum(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_builtin_ctype_named("ctype_alnum", args, context, scope, values)
}

/// Returns the PHP boolean result for `ctype_alnum(...)` from one evaluated value of any type.
pub(in crate::interpreter) fn eval_ctype_alnum_result(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_ctype_named_result("ctype_alnum", value, context, values)
}

/// Evaluates a named PHP `ctype_*` predicate over one eval argument expression.
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
    eval_ctype_named_result(name, value, context, values)
}

/// Returns the PHP boolean result for one named `ctype_*` check over any eval value.
///
/// A string is classified byte by byte. Every other value first raises php-src 8.1+'s
/// "will be interpreted as string in the future" deprecation. An int in 0..=255 is then
/// classified as that byte, an int in -128..=-1 as the byte `value + 256`, and any other int
/// as its decimal string would be; every other value is `false`.
pub(in crate::interpreter) fn eval_ctype_named_result(
    name: &str,
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    if tag == EVAL_TAG_STRING {
        let bytes = values.string_bytes(value)?;
        let mut matches = !bytes.is_empty();
        for byte in bytes {
            if !eval_ctype_byte_matches(name, byte)? {
                matches = false;
                break;
            }
        }
        return values.bool_value(matches);
    }
    // The deprecation names a bool by its type, unlike a TypeError's `true`/`false`.
    let given = if tag == EVAL_TAG_BOOL {
        "bool".to_string()
    } else {
        eval_given_type_name(value, context, values)?
    };
    values.warning(&format!(
        "Deprecated: {name}(): Argument of type {given} will be interpreted as string in the future\n"
    ))?;
    if tag != EVAL_TAG_INT {
        return values.bool_value(false);
    }
    let integer = eval_int_value(value, values)?;
    let matches = match integer {
        0..=255 => eval_ctype_byte_matches(name, integer as u8)?,
        -128..=-1 => eval_ctype_byte_matches(name, (integer + 256) as u8)?,
        256.. => eval_ctype_accepts_digit_string(name),
        _ => eval_ctype_accepts_negative_digit_string(name),
    };
    values.bool_value(matches)
}

/// Returns php-src's answer for a non-negative int above 255, whose decimal digits it classifies.
fn eval_ctype_accepts_digit_string(name: &str) -> bool {
    matches!(name, "ctype_alnum" | "ctype_digit" | "ctype_graph" | "ctype_print" | "ctype_xdigit")
}

/// Returns php-src's answer for an int below -128, whose decimal form starts with `-`.
fn eval_ctype_accepts_negative_digit_string(name: &str) -> bool {
    matches!(name, "ctype_graph" | "ctype_print")
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
        "ctype_cntrl" => Ok(byte.is_ascii_control()),
        "ctype_graph" => Ok(byte.is_ascii_graphic()),
        "ctype_lower" => Ok(byte.is_ascii_lowercase()),
        "ctype_print" => Ok(byte.is_ascii_graphic() || byte == b' '),
        "ctype_punct" => Ok(byte.is_ascii_punctuation()),
        "ctype_space" => Ok(matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')),
        "ctype_upper" => Ok(byte.is_ascii_uppercase()),
        "ctype_xdigit" => Ok(byte.is_ascii_hexdigit()),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
