//! Purpose:
//! Shared eval implementation for PHP's complete `ctype_*` family, plus the
//! `ctype_alnum` registry entry.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` through the direct and evaluated-argument
//!   Ctype hooks.
//!
//! Key details:
//! - Strings scan every byte and reject the empty string.
//! - Non-string arguments emit php-src's `E_DEPRECATED` and then apply the deprecated
//!   integer window/wrap/allow_digits/allow_minus fallback.
//! - Object deprecations name the concrete class, matching `zend_zval_type_name()`.

eval_builtin! {
    contract: "ctype_alnum",
    area: String,
    direct: Ctype,
    values: Ctype,
}

use super::super::super::*;

/// Evaluates one named PHP `ctype_*` predicate over one eval expression.
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

/// Returns the PHP boolean result for one named `ctype_*` check.
pub(in crate::interpreter) fn eval_ctype_named_result(
    name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let matches = if values.type_tag(value)? == EVAL_TAG_STRING {
        let bytes = values.string_bytes(value)?;
        let mut matches = !bytes.is_empty();
        for byte in bytes {
            if !eval_ctype_byte_matches(name, byte)? {
                matches = false;
                break;
            }
        }
        matches
    } else {
        let type_name = eval_ctype_type_name(value, values)?;
        eval_dispatch_php_error(
            &format!(
                "{name}(): Argument of type {type_name} will be interpreted as string in the future"
            ),
            E_DEPRECATED,
            context,
            values,
        )?;
        if values.type_tag(value)? == EVAL_TAG_INT {
            eval_ctype_integer_fallback(name, eval_int_value(value, values)?)?
        } else {
            false
        }
    };
    values.bool_value(matches)
}

/// Returns `zend_zval_type_name()`'s spelling for one evaluated runtime value.
fn eval_ctype_type_name(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let tag = values.type_tag(value)?;
    Ok(match tag {
        EVAL_TAG_INT => "int".to_string(),
        EVAL_TAG_FLOAT => "float".to_string(),
        EVAL_TAG_BOOL => "bool".to_string(),
        EVAL_TAG_NULL => "null".to_string(),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array".to_string(),
        EVAL_TAG_RESOURCE => "resource".to_string(),
        EVAL_TAG_OBJECT => {
            let class = values.object_class_name(value)?;
            let bytes = values.string_bytes(class);
            values.release(class)?;
            String::from_utf8_lossy(&bytes?).into_owned()
        }
        _ => "unknown".to_string(),
    })
}

/// Applies php-src's deprecated integer fallback after the deprecation was dispatched.
fn eval_ctype_integer_fallback(name: &str, code: i64) -> Result<bool, EvalStatus> {
    if (0..=255).contains(&code) {
        return eval_ctype_byte_matches(name, code as u8);
    }
    if (-128..0).contains(&code) {
        return eval_ctype_byte_matches(name, (code + 256) as u8);
    }
    if code >= 0 {
        return Ok(eval_ctype_allows_digits(name));
    }
    Ok(eval_ctype_allows_minus(name))
}

/// Reports whether a positive out-of-window integer is accepted as a digit-like value.
fn eval_ctype_allows_digits(name: &str) -> bool {
    matches!(
        name,
        "ctype_alnum" | "ctype_digit" | "ctype_graph" | "ctype_print" | "ctype_xdigit"
    )
}

/// Reports whether a negative out-of-window integer is accepted as a minus sign.
fn eval_ctype_allows_minus(name: &str) -> bool {
    matches!(name, "ctype_graph" | "ctype_print")
}

/// Checks one byte against the selected PHP ASCII character class.
pub(in crate::interpreter) fn eval_ctype_byte_matches(
    name: &str,
    byte: u8,
) -> Result<bool, EvalStatus> {
    match name {
        "ctype_alnum" => Ok(byte.is_ascii_alphanumeric()),
        "ctype_alpha" => Ok(byte.is_ascii_alphabetic()),
        "ctype_cntrl" => Ok(byte.is_ascii_control()),
        "ctype_digit" => Ok(byte.is_ascii_digit()),
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
