//! Purpose:
//! Eval binding and scalar validation implementation for PHP's `filter_var`.
//!
//! Called from:
//! - The declarative eval builtin registry through direct and value hooks.
//!
//! Key details:
//! - This preserves the scalar filter subset already supported by the AOT backend, with php's
//!   `options` array (`flags`, `min_range`, `max_range`, `default`) and the IP range flags. Filters
//!   it does not implement (URL, e-mail, MAC, domain, callback, regexp, sanitize) refuse with an
//!   unsupported-construct fatal instead of answering `false`.
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
const FILTER_FLAG_NO_PRIV_RANGE: i64 = 8_388_608;
const FILTER_FLAG_NO_RES_RANGE: i64 = 4_194_304;
const FILTER_FLAG_GLOBAL_RANGE: i64 = 536_870_912;

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

/// The `options` argument of one `filter_var()` call, decoded.
///
/// php takes either an int (the flags) or an array carrying `flags` and an `options` sub-array
/// whose `min_range`, `max_range` and `default` the scalar filters honour. Every cell held here
/// is OWNED, given back by [`EvalFilterOptions::release`].
struct EvalFilterOptions {
    flags: i64,
    min_range: Option<RuntimeCellHandle>,
    max_range: Option<RuntimeCellHandle>,
    default: Option<RuntimeCellHandle>,
}

impl EvalFilterOptions {
    /// Gives back every cell this decoding owns.
    fn release(self, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
        let mut result = Ok(());
        for cell in [self.min_range, self.max_range, self.default].into_iter().flatten() {
            let released = values.release(cell);
            if result.is_ok() {
                result = released;
            }
        }
        result
    }
}

/// Reads `$options[name]` as an owned cell, or None when the key is absent.
fn eval_filter_option(
    options: RuntimeCellHandle,
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let key = values.string(name)?;
    let exists = values.array_key_exists(key, options);
    let present = exists.and_then(|exists| {
        let present = values.truthy(exists);
        values.release(exists)?;
        present
    });
    let value = match present {
        Ok(true) => values.array_get(options, key).map(Some),
        Ok(false) => Ok(None),
        Err(status) => Err(status),
    };
    values.release(key)?;
    value
}

/// Reads the `options` sub-array of php's options array into `decoded`.
fn eval_filter_decode_sub_options(
    sub: RuntimeCellHandle,
    decoded: &mut EvalFilterOptions,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if matches!(values.type_tag(sub)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        decoded.min_range = eval_filter_option(sub, "min_range", values)?;
        decoded.max_range = eval_filter_option(sub, "max_range", values)?;
        decoded.default = eval_filter_option(sub, "default", values)?;
    }
    Ok(())
}

/// Reads the `flags` and `options` entries of php's options array into `decoded`.
fn eval_filter_decode_array(
    options: RuntimeCellHandle,
    decoded: &mut EvalFilterOptions,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(flags) = eval_filter_option(options, "flags", values)? {
        let flags_value = eval_int_value(flags, values);
        values.release(flags)?;
        decoded.flags = flags_value?;
    }
    if let Some(sub) = eval_filter_option(options, "options", values)? {
        let read = eval_filter_decode_sub_options(sub, decoded, values);
        values.release(sub)?;
        read?;
    }
    Ok(())
}

/// Decodes the third `filter_var()` argument: an int of flags or php's options array.
fn eval_filter_decode_options(
    options: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalFilterOptions, EvalStatus> {
    let mut decoded = EvalFilterOptions { flags: 0, min_range: None, max_range: None, default: None };
    let Some(options) = options else {
        return Ok(decoded);
    };
    if !matches!(values.type_tag(options)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        decoded.flags = eval_int_value(options, values)?;
        return Ok(decoded);
    }
    match eval_filter_decode_array(options, &mut decoded, values) {
        Ok(()) => Ok(decoded),
        Err(status) => {
            let _ = decoded.release(values);
            Err(status)
        }
    }
}

/// Evaluates `filter_var()` after PHP argument binding has materialized values.
pub(in crate::interpreter) fn eval_filter_var_values_result(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (value, filter, options) = match args {
        [value] => (*value, FILTER_DEFAULT, None),
        [value, filter] => (*value, eval_int_value(*filter, values)?, None),
        [value, filter, options] => (*value, eval_int_value(*filter, values)?, Some(*options)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    // Only the filters implemented below exist here: answering `false` for URL, e-mail, MAC,
    // domain, callback, regexp or sanitize filters would be a silent wrong answer, so they stay
    // the loud refusal they were before this binding existed.
    if !matches!(
        filter,
        FILTER_DEFAULT | FILTER_VALIDATE_BOOL | FILTER_VALIDATE_INT | FILTER_VALIDATE_FLOAT | FILTER_VALIDATE_IP
    ) {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let decoded = eval_filter_decode_options(options, values)?;
    let null_on_failure = decoded.flags & FILTER_NULL_ON_FAILURE != 0;
    let outcome = match filter {
        FILTER_DEFAULT => eval_filter_default(value, null_on_failure, values),
        FILTER_VALIDATE_BOOL => eval_filter_bool(value, null_on_failure, values),
        FILTER_VALIDATE_INT => eval_filter_int(value, &decoded, null_on_failure, values),
        FILTER_VALIDATE_FLOAT => eval_filter_float(value, &decoded, null_on_failure, values),
        _ => eval_filter_ip(value, decoded.flags, null_on_failure, values),
    };
    // A `default` option replaces a failed validation, ahead of FILTER_NULL_ON_FAILURE.
    let result = match (outcome, decoded.default) {
        (Ok(answer), Some(default)) if eval_filter_is_failure(answer, null_on_failure, values)? => {
            values.release(answer)?;
            values.retain(default)
        }
        (outcome, _) => outcome,
    };
    let released = decoded.release(values);
    let result = result?;
    released?;
    Ok(result)
}

/// Reports whether a filter answer is the failure sentinel (false, or null under NULL_ON_FAILURE).
fn eval_filter_is_failure(
    answer: RuntimeCellHandle,
    null_on_failure: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let tag = values.type_tag(answer)?;
    Ok(if null_on_failure {
        tag == EVAL_TAG_NULL
    } else {
        tag == EVAL_TAG_BOOL && !values.truthy(answer)?
    })
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
    options: &EvalFilterOptions,
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
            Some(result) if eval_filter_in_range(result as f64, options, values)? => values.int(result),
            _ => eval_filter_failure(null_on_failure, values),
        },
    }
}

/// Validates a finite float using the decimal grammar accepted by ext/filter.
fn eval_filter_float(
    value: RuntimeCellHandle,
    options: &EvalFilterOptions,
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
            Some(result) if eval_filter_in_range(result, options, values)? => values.float(result),
            _ => eval_filter_failure(null_on_failure, values),
        },
    }
}

/// Applies the `min_range` and `max_range` options to one validated number.
fn eval_filter_in_range(
    number: f64,
    options: &EvalFilterOptions,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for (bound, is_minimum) in [(options.min_range, true), (options.max_range, false)] {
        let Some(bound) = bound else {
            continue;
        };
        let text = values.string_bytes(bound)?;
        let Some(limit) = std::str::from_utf8(&text).ok().and_then(|text| text.trim().parse::<f64>().ok()) else {
            continue;
        };
        if (is_minimum && number < limit) || (!is_minimum && number > limit) {
            return Ok(false);
        }
    }
    Ok(true)
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
    let global = options & FILTER_FLAG_GLOBAL_RANGE != 0;
    let no_private = global || options & FILTER_FLAG_NO_PRIV_RANGE != 0;
    let no_reserved = global || options & FILTER_FLAG_NO_RES_RANGE != 0;
    let excluded = match address {
        IpAddr::V4(address) => eval_filter_ipv4_excluded(address.octets(), no_private, no_reserved, global),
        IpAddr::V6(address) => eval_filter_ipv6_excluded(address.segments(), no_private, no_reserved, global),
    };
    if excluded {
        return eval_filter_failure(null_on_failure, values);
    }
    values.retain(value)
}

/// Applies `FILTER_FLAG_NO_PRIV_RANGE`, `NO_RES_RANGE` and `GLOBAL_RANGE` to an IPv4 address.
///
/// The ranges are the ones php 8.5's ext/filter rejects, measured against `php -n`.
fn eval_filter_ipv4_excluded(octets: [u8; 4], no_private: bool, no_reserved: bool, global: bool) -> bool {
    let [a, b, c, _] = octets;
    let private = a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168);
    let reserved = a == 0 || a >= 240 || a == 127 || (a == 169 && b == 254);
    let special = (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113);
    (no_private && private) || (no_reserved && reserved) || (global && special)
}

/// Applies the same range flags to an IPv6 address, from its eight 16-bit groups.
fn eval_filter_ipv6_excluded(groups: [u16; 8], no_private: bool, no_reserved: bool, global: bool) -> bool {
    let private = groups[0] & 0xfe00 == 0xfc00;
    let loopback_or_unspecified = groups[..7].iter().all(|group| *group == 0) && groups[7] <= 1;
    let mapped = groups[..5].iter().all(|group| *group == 0) && groups[5] == 0xffff;
    let reserved = loopback_or_unspecified || mapped || (0xfe80..=0xfebf).contains(&groups[0]);
    let special = (groups[0] == 0x0100 && groups[1..4].iter().all(|group| *group == 0))
        || (groups[0] == 0x2001 && (groups[1] < 0x0200 || groups[1] == 0x0db8))
        || groups[0] == 0x2002;
    (no_private && private) || (no_reserved && reserved) || (global && special)
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
