//! Purpose:
//! Eval binding and scalar validation implementation for PHP's `filter_var`.
//!
//! Called from:
//! - The declarative eval builtin registry through direct and value hooks.
//!
//! Key details:
//! - This preserves the scalar filter subset already supported by the AOT backend.
//! - Validation parses PHP-filter input rather than reusing permissive numeric casts.

use std::net::IpAddr;

use super::super::super::*;

const FILTER_DEFAULT: i64 = 516;
const FILTER_VALIDATE_INT: i64 = 257;
const FILTER_VALIDATE_BOOL: i64 = 258;
const FILTER_VALIDATE_FLOAT: i64 = 259;
const FILTER_VALIDATE_IP: i64 = 275;
const FILTER_NULL_ON_FAILURE: i64 = 134_217_728;
const FILTER_FLAG_IPV4: i64 = 1_048_576;
const FILTER_FLAG_IPV6: i64 = 2_097_152;

eval_builtin! {
    contract: "filter_var",
    area: Types,
    direct: FilterVar,
    values: FilterVar,
}

/// Evaluates `filter_var()` while preserving PHP argument evaluation order.
pub(in crate::interpreter) fn eval_builtin_filter_var(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_filter_var_values_result(&evaluated, values)
}

/// Evaluates `filter_var()` after PHP argument binding has materialized values.
pub(in crate::interpreter) fn eval_filter_var_values_result(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (value, filter, options) = match args {
        [value] => (*value, FILTER_DEFAULT, 0),
        [value, filter] => (*value, eval_int_value(*filter, values)?, 0),
        [value, filter, options] => (
            *value,
            eval_int_value(*filter, values)?,
            eval_int_value(*options, values)?,
        ),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let null_on_failure = options & FILTER_NULL_ON_FAILURE != 0;
    match filter {
        FILTER_DEFAULT => eval_filter_default(value, null_on_failure, values),
        FILTER_VALIDATE_BOOL => eval_filter_bool(value, null_on_failure, values),
        FILTER_VALIDATE_INT => eval_filter_int(value, null_on_failure, values),
        FILTER_VALIDATE_FLOAT => eval_filter_float(value, null_on_failure, values),
        FILTER_VALIDATE_IP => eval_filter_ip(value, options, null_on_failure, values),
        _ => eval_filter_failure(null_on_failure, values),
    }
}

/// Returns the default unsafe-raw result for scalar input and a validation failure for arrays.
fn eval_filter_default(
    value: RuntimeCellHandle,
    null_on_failure: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => eval_filter_failure(null_on_failure, values),
        _ => values.retain(value),
    }
}

/// Validates one PHP boolean filter token after PHP's ext/filter whitespace trim.
fn eval_filter_bool(
    value: RuntimeCellHandle,
    null_on_failure: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_NULL => values.bool_value(false),
        EVAL_TAG_BOOL => {
            let result = values.truthy(value)?;
            values.bool_value(result)
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT | EVAL_TAG_RESOURCE => {
            eval_filter_failure(null_on_failure, values)
        }
        _ => match eval_filter_bool_token(&values.string_bytes(value)?) {
            Some(result) => values.bool_value(result),
            None => eval_filter_failure(null_on_failure, values),
        },
    }
}

/// Validates an integer using ext/filter's decimal-only grammar.
fn eval_filter_int(
    value: RuntimeCellHandle,
    null_on_failure: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_NULL | EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT | EVAL_TAG_RESOURCE => {
            eval_filter_failure(null_on_failure, values)
        }
        EVAL_TAG_BOOL if !values.truthy(value)? => eval_filter_failure(null_on_failure, values),
        EVAL_TAG_BOOL => values.int(1),
        _ => match eval_filter_int_token(&values.string_bytes(value)?) {
            Some(result) => values.int(result),
            None => eval_filter_failure(null_on_failure, values),
        },
    }
}

/// Validates a finite float using the decimal grammar accepted by ext/filter.
fn eval_filter_float(
    value: RuntimeCellHandle,
    null_on_failure: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_NULL | EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT | EVAL_TAG_RESOURCE => {
            eval_filter_failure(null_on_failure, values)
        }
        EVAL_TAG_BOOL if !values.truthy(value)? => eval_filter_failure(null_on_failure, values),
        EVAL_TAG_BOOL => values.float(1.0),
        _ => match eval_filter_float_token(&values.string_bytes(value)?) {
            Some(result) => values.float(result),
            None => eval_filter_failure(null_on_failure, values),
        },
    }
}

/// Validates a literal IP address and applies the AOT-supported family flags.
fn eval_filter_ip(
    value: RuntimeCellHandle,
    options: i64,
    null_on_failure: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_STRING {
        return eval_filter_failure(null_on_failure, values);
    }
    let bytes = values.string_bytes(value)?;
    let Ok(address) = std::str::from_utf8(&bytes).ok().and_then(|text| text.parse::<IpAddr>().ok()).ok_or(()) else {
        return eval_filter_failure(null_on_failure, values);
    };
    let ipv4_only = options & FILTER_FLAG_IPV4 != 0 && options & FILTER_FLAG_IPV6 == 0;
    let ipv6_only = options & FILTER_FLAG_IPV6 != 0 && options & FILTER_FLAG_IPV4 == 0;
    if (ipv4_only && !address.is_ipv4()) || (ipv6_only && !address.is_ipv6()) {
        return eval_filter_failure(null_on_failure, values);
    }
    values.retain(value)
}

/// Produces PHP false, or PHP null under `FILTER_NULL_ON_FAILURE`.
fn eval_filter_failure(
    null_on_failure: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if null_on_failure {
        values.null()
    } else {
        values.bool_value(false)
    }
}

/// Trims exactly the ASCII whitespace set used by PHP's filter extension.
fn eval_filter_trim(bytes: &[u8]) -> &[u8] {
    let first = bytes.iter().position(|byte| !matches!(byte, b'\t' | b'\n' | b'\x0b' | b'\r' | b' '));
    let Some(first) = first else {
        return &[];
    };
    let last = bytes.iter().rposition(|byte| !matches!(byte, b'\t' | b'\n' | b'\x0b' | b'\r' | b' ')).expect("first non-whitespace byte has a last position");
    &bytes[first..=last]
}

/// Returns the bool represented by a PHP filter token, if it is valid.
fn eval_filter_bool_token(bytes: &[u8]) -> Option<bool> {
    let bytes = eval_filter_trim(bytes);
    if matches!(bytes, b"1") || bytes.eq_ignore_ascii_case(b"true") || bytes.eq_ignore_ascii_case(b"on") || bytes.eq_ignore_ascii_case(b"yes") {
        return Some(true);
    }
    if bytes.is_empty() || matches!(bytes, b"0") || bytes.eq_ignore_ascii_case(b"false") || bytes.eq_ignore_ascii_case(b"off") || bytes.eq_ignore_ascii_case(b"no") {
        return Some(false);
    }
    None
}

/// Parses PHP's decimal-only `FILTER_VALIDATE_INT` lexical form.
fn eval_filter_int_token(bytes: &[u8]) -> Option<i64> {
    let bytes = eval_filter_trim(bytes);
    let (negative, digits) = match bytes.first().copied() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if digits.len() > 1 && digits[0] == b'0' {
        return None;
    }
    let magnitude = digits.iter().try_fold(0_u64, |value, byte| {
        value.checked_mul(10)?.checked_add(u64::from(byte - b'0'))
    })?;
    if negative {
        (magnitude <= (i64::MAX as u64) + 1).then_some((magnitude as i64).wrapping_neg())
    } else {
        (magnitude <= i64::MAX as u64).then_some(magnitude as i64)
    }
}

/// Parses PHP's decimal/scientific `FILTER_VALIDATE_FLOAT` lexical form.
fn eval_filter_float_token(bytes: &[u8]) -> Option<f64> {
    let bytes = eval_filter_trim(bytes);
    let mut index = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let mantissa_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    let integer_digits = index - mantissa_start;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    let has_mantissa_digit = index > mantissa_start + usize::from(bytes.get(mantissa_start) == Some(&b'.'));
    if !has_mantissa_digit {
        return None;
    }
    if bytes.get(index).is_some_and(|byte| matches!(byte, b'e' | b'E')) {
        index += 1;
        if bytes.get(index).is_some_and(|byte| matches!(byte, b'+' | b'-')) {
            index += 1;
        }
        let exponent_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == exponent_start {
            return None;
        }
    }
    if index != bytes.len() || integer_digits == 0 && !bytes.contains(&b'.') {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse::<f64>().ok().filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_tokens_follow_php_filter_whitespace_and_case_rules() {
        assert_eq!(eval_filter_bool_token(b" \tYES\r"), Some(true));
        assert_eq!(eval_filter_bool_token(b"\x0cyes"), None);
        assert_eq!(eval_filter_bool_token(b""), Some(false));
        assert_eq!(eval_filter_bool_token(b"invalid"), None);
    }

    #[test]
    fn integer_tokens_reject_leading_zeroes_and_overflow() {
        assert_eq!(eval_filter_int_token(b" -42 "), Some(-42));
        assert_eq!(eval_filter_int_token(b"012"), None);
        assert_eq!(eval_filter_int_token(b"9223372036854775808"), None);
    }

    #[test]
    fn float_tokens_require_a_finite_decimal_or_exponent_form() {
        assert_eq!(eval_filter_float_token(b"1.5e2"), Some(150.0));
        assert_eq!(eval_filter_float_token(b"1e"), None);
        assert_eq!(eval_filter_float_token(b"NaN"), None);
    }
}
