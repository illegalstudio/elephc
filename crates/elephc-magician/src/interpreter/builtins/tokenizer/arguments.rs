//! Purpose:
//! Applies tokenizer's PHP internal scalar argument rules before values are narrowed.
//!
//! Called from:
//! - Tokenizer builtin hooks and the native PhpToken::tokenize binding adapter.
//!
//! Key details:
//! - Integer-form strings preserve the complete i64 range before float fallback.
//! - Weak null coercion remains valid and emits the PHP internal-function deprecation.
//! - Only a per-call signature copy is widened; reflection retains the declared types.

use super::*;

/// Recognizes the inherited internal factory without changing user overrides.
fn is_tokenizer_factory(owner: &str, method: &str) -> bool {
    owner.trim_start_matches('\\').eq_ignore_ascii_case("PhpToken")
        && method.eq_ignore_ascii_case("tokenize")
}

/// Preserves original scalar operands until the tokenizer's ZPP-compatible validation.
pub(in crate::interpreter) fn tokenizer_native_binding_signature(
    owner: &str, method: &str, signature: Option<NativeCallableSignature>,
) -> Option<NativeCallableSignature> {
    let mut signature = signature?;
    if is_tokenizer_factory(owner, method) {
        for index in 0..2 {
            signature.set_param_type(index, EvalParameterType::new(vec![EvalParameterTypeVariant::Mixed], true));
        }
    }
    Some(signature)
}

/// Replaces bound operands with validated native representations, preserving their owners.
pub(in crate::interpreter) fn normalize_tokenizer_native_arguments(
    owner: &str, method: &str, args: &mut [BoundMethodArg],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !is_tokenizer_factory(owner, method) { return Ok(()); }
    let code = args.first_mut().ok_or(EvalStatus::RuntimeFatal)?;
    if values.type_tag(code.value)? != EVAL_TAG_STRING {
        let bytes = tokenizer_source(code.value, "PhpToken::tokenize", context, values)?;
        let normalized = values.string_bytes_value(&bytes)?;
        let previous = std::mem::replace(&mut code.value, normalized);
        release_expr_result(previous, context, values)?;
    }
    if let Some(flags) = args.get_mut(1) {
        if values.type_tag(flags.value)? != EVAL_TAG_INT {
            let integer = tokenizer_int(flags.value, "PhpToken::tokenize", 2, "flags", context, values)?;
            let normalized = values.int(integer)?;
            let previous = std::mem::replace(&mut flags.value, normalized);
            release_expr_result(previous, context, values)?;
        }
    }
    Ok(())
}

/// Converts the source using weak internal string rules, including Stringable objects.
pub(super) fn tokenizer_source(
    code: RuntimeCellHandle, name: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let tag = values.type_tag(code)?;
    if matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_RESOURCE | EVAL_TAG_CALLABLE)
        || tag == EVAL_TAG_OBJECT && !eval_object_has_to_string(code, context, values)? {
        let given = eval_given_type_name(code, context, values)?;
        return eval_throw_type_error(&format!("{name}(): Argument #1 ($code) must be of type string, {given} given"), context, values);
    }
    if tag == EVAL_TAG_NULL {
        values.warning(&format!("Deprecated: {name}(): Passing null to parameter #1 ($code) of type string is deprecated\n"))?;
    }
    if tag == EVAL_TAG_OBJECT {
        let text = eval_string_context_value(code, context, values)?;
        let result = values.string_bytes(text);
        let released = values.release(text);
        let bytes = result?;
        released?;
        return Ok(bytes);
    }
    values.string_bytes(code)
}

/// Converts an internal integer argument without applying explicit-cast overflow rules.
pub(super) fn tokenizer_int(
    value: RuntimeCellHandle, name: &str, number: usize, parameter: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let tag = values.type_tag(value)?;
    if tag == EVAL_TAG_STRING {
        let bytes = values.string_bytes(value)?;
        if let Ok(source) = std::str::from_utf8(&bytes) {
            // PHP numeric strings also accept vertical tab, unlike Rust's ASCII whitespace set.
            let numeric = source.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}'));
            if let Ok(integer) = numeric.parse::<i64>() { return Ok(integer); }
            if let Ok(float) = numeric.parse::<f64>() {
                if fits_php_int(float) {
                    if float.trunc() != float {
                        values.warning(&format!("Deprecated: Implicit conversion from float-string \"{source}\" to int loses precision\n"))?;
                    }
                    return Ok(float.trunc() as i64);
                }
            }
        }
    } else if tag == EVAL_TAG_FLOAT {
        let float = f64::from_bits(values.raw_value_word(value)?);
        if fits_php_int(float) { return eval_int_value(value, values); }
    } else if tag == EVAL_TAG_INT || tag == EVAL_TAG_BOOL {
        return eval_int_value(value, values);
    } else if tag == EVAL_TAG_NULL {
        values.warning(&format!("Deprecated: {name}(): Passing null to parameter #{number} (${parameter}) of type int is deprecated\n"))?;
        return Ok(0);
    }
    let given = eval_given_type_name(value, context, values)?;
    eval_throw_type_error(&format!("{name}(): Argument #{number} (${parameter}) must be of type int, {given} given"), context, values)
}

/// Mirrors Zend's inclusive lower bound and exclusive rounded upper bound for doubles.
fn fits_php_int(value: f64) -> bool {
    value.is_finite() && value >= i64::MIN as f64 && value < -(i64::MIN as f64)
}
