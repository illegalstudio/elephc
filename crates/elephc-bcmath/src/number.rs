//! Purpose:
//! Implements immutable BcMath\Number decimal semantics independently of process scale.
//!
//! Called from:
//! - Number C ABI entries used by compiler-generated methods and Magician.
//!
//! Key details:
//! - Full input scale includes trailing zeroes; automatic expansion is at most ten digits.
//! - Results are normalized decimal strings whose fractional length is the object's scale.

use crate::{
    bc_add, bc_ceil, bc_comp, bc_div, bc_divmod, bc_floor, bc_mod, bc_mul, bc_pow,
    bc_powmod, bc_round, bc_sqrt, bc_sub, format_bcmath_number, parse_bcmath_number,
    BcError, BcNum, BCMATH_ERR_POW_RANGE, BCMATH_ERR_SCALE_RANGE,
};

/// Returns a normalized constructor value without discarding the supplied scale.
pub fn number_create(value: &str) -> Result<String, BcError> {
    let parsed = operand(value, "__construct", 1, "num")?;
    format_bcmath_number(&parsed, parsed.scale)
}

/// Runs one binary value operation with the class's automatic-scale policy.
pub fn number_binary(
    method: &'static str,
    left: &str,
    right: &str,
    scale: Option<i64>,
) -> Result<String, BcError> {
    let lhs = operand(left, method, 1, "num")?;
    let rhs_name = if method == "pow" { "exponent" } else { "num" };
    let rhs = operand(right, method, 1, rhs_name)?;
    let explicit = checked_scale(scale, method, 2)?;
    let mut expanded = false;
    let automatic = match method {
        "mul" => i64::from(lhs.scale) + i64::from(rhs.scale),
        "div" => {
            expanded = true;
            i64::from(lhs.scale) + 10
        }
        "pow" => {
            let exponent = integral_exponent(right, method)?;
            if exponent < 0 {
                expanded = true;
                i64::from(lhs.scale) + 10
            } else {
                if explicit.is_some() { 0 } else {
                    i64::from(lhs.scale).checked_mul(exponent).ok_or_else(result_scale_error)?
                }
            }
        }
        _ => i64::from(lhs.scale.max(rhs.scale)),
    };
    let result_scale = match explicit {
        Some(scale) => scale,
        None => i32::try_from(automatic).map_err(|_| result_scale_error())?,
    };
    let operation = match method {
        "add" => bc_add,
        "sub" => bc_sub,
        "mul" => bc_mul,
        "div" => bc_div,
        "mod" => bc_mod,
        "pow" => bc_pow,
        _ => unreachable!("typed Number binary operation"),
    };
    let mut result = operation(left, right, Some(i64::from(result_scale)))
        .map_err(|error| contextualize(error, method))?;
    if expanded && explicit.is_none() {
        trim_expansion(&mut result, lhs.scale);
    }
    Ok(result)
}

/// Compares values at the explicit scale or at their full precision.
pub fn number_compare(left: &str, right: &str, scale: Option<i64>) -> Result<i32, BcError> {
    let lhs = operand(left, "compare", 1, "num")?;
    let rhs = operand(right, "compare", 1, "num")?;
    let scale = checked_scale(scale, "compare", 2)?.unwrap_or(lhs.scale.max(rhs.scale));
    bc_comp(left, right, Some(i64::from(scale)))
}

/// Returns an integer quotient and a remainder preserving automatic operand scale.
pub fn number_divmod(
    left: &str,
    right: &str,
    scale: Option<i64>,
) -> Result<(String, String), BcError> {
    let lhs = operand(left, "divmod", 1, "num")?;
    let rhs = operand(right, "divmod", 1, "num")?;
    let scale = checked_scale(scale, "divmod", 2)?.unwrap_or(lhs.scale.max(rhs.scale));
    bc_divmod(left, right, Some(i64::from(scale)))
        .map_err(|error| contextualize(error, "divmod"))
}

/// Computes modular powers, whose omitted result scale is always zero.
pub fn number_powmod(
    base: &str,
    exponent: &str,
    modulus: &str,
    scale: Option<i64>,
) -> Result<String, BcError> {
    operand(base, "powmod", 1, "num")?;
    operand(exponent, "powmod", 1, "exponent")?;
    operand(modulus, "powmod", 2, "modulus")?;
    let scale = checked_scale(scale, "powmod", 3)?.unwrap_or(0);
    crate::pow::parse_integral_operand(base, "bcpowmod", 1, "num")
        .map_err(|error| contextualize(error, "powmod"))?;
    let integral_exponent = crate::pow::parse_integral_operand(exponent, "bcpowmod", 2, "exponent")
        .map_err(|error| contextualize(error, "powmod"))?;
    if integral_exponent.negative {
        return Err(contextualize(BcError::PowModNegativeExponent, "powmod"));
    }
    bc_powmod(base, exponent, modulus, Some(i64::from(scale)))
        .map_err(|error| contextualize(error, "powmod"))
}

/// Computes a square root with optional ten-digit automatic expansion.
pub fn number_sqrt(value: &str, scale: Option<i64>) -> Result<String, BcError> {
    let parsed = operand(value, "sqrt", 1, "num")?;
    let explicit = checked_scale(scale, "sqrt", 1)?;
    let scale = match explicit {
        Some(scale) => scale,
        None => parsed.scale.checked_add(10).ok_or_else(result_scale_error)?,
    };
    let mut result = bc_sqrt(value, Some(i64::from(scale)))
        .map_err(|error| contextualize(error, "sqrt"))?;
    if explicit.is_none() {
        trim_expansion(&mut result, parsed.scale);
    }
    Ok(result)
}

/// Rounds to an integer boundary or to a signed decimal precision.
pub fn number_rounding(
    method: &'static str,
    value: &str,
    precision: i64,
    mode: i64,
) -> Result<String, BcError> {
    operand(value, method, 1, "num")?;
    let result = match method {
        "ceil" => bc_ceil(value),
        "floor" => bc_floor(value),
        "round" => bc_round(value, precision, mode),
        _ => unreachable!("typed Number rounding operation"),
    };
    result.map_err(|error| contextualize(error, method))
}

/// Parses one operand before validating optional arguments, as PHP methods do.
fn operand(value: &str, method: &'static str, position: u32, name: &str) -> Result<BcNum, BcError> {
    parse_bcmath_number(value).map_err(|error| BcError::Number {
        status: error.status_code(),
        message: format!("BcMath\\Number::{method}(): Argument #{position} (${name}) is not well-formed"),
    })
}

/// Checks a method's nullable scale without consulting procedural global state.
fn checked_scale(scale: Option<i64>, method: &str, position: u32) -> Result<Option<i32>, BcError> {
    scale.map(|value| {
        i32::try_from(value).ok().filter(|value| *value >= 0).ok_or_else(|| BcError::Number {
            status: BCMATH_ERR_SCALE_RANGE,
            message: format!("BcMath\\Number::{method}(): Argument #{position} ($scale) must be between 0 and 2147483647"),
        })
    }).transpose()
}

/// Resolves an integral exponent without losing significant fractional digits.
fn integral_exponent(value: &str, method: &str) -> Result<i64, BcError> {
    let number = parse_bcmath_number(value)?;
    let integer = format_bcmath_number(&number, 0)?;
    if bc_comp(value, &integer, Some(i64::from(number.scale)))? != 0 {
        return Err(BcError::Number {
            status: crate::BCMATH_ERR_POW_FRACTIONAL,
            message: format!("BcMath\\Number::{method}(): Argument #1 ($exponent) exponent cannot have a fractional part"),
        });
    }
    integer.parse::<i64>().map_err(|_| BcError::Number {
        status: BCMATH_ERR_POW_RANGE,
        message: format!("BcMath\\Number::{method}(): Argument #1 ($exponent) exponent is too large"),
    })
}

/// Removes only optional expansion zeroes, preserving the receiver's original scale.
fn trim_expansion(value: &mut String, minimum_scale: i32) {
    let Some(point) = value.find('.') else { return };
    let minimum_len = point + 1 + minimum_scale as usize;
    while value.len() > minimum_len && value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
}

/// Reports automatic scale overflow separately from an invalid supplied scale.
fn result_scale_error() -> BcError {
    BcError::Number {
        status: BCMATH_ERR_SCALE_RANGE,
        message: "scale of the result is too large".to_string(),
    }
}

/// Translates procedural engine errors into PHP's class-method diagnostics.
fn contextualize(error: BcError, method: &str) -> BcError {
    let message = match &error {
        BcError::PowFractional { arg_name, .. } => {
            let position = if *arg_name == "modulus" { 2 } else { 1 };
            if method == "powmod" && *arg_name == "num" {
                "Base number cannot have a fractional part".to_string()
            } else {
                let detail = if method == "pow" { "exponent " } else { "" };
                format!("BcMath\\Number::{method}(): Argument #{position} (${arg_name}) {detail}cannot have a fractional part")
            }
        }
        BcError::SqrtNegative => "Base number must be greater than or equal to 0".to_string(),
        BcError::PowRange => format!("BcMath\\Number::{method}(): Argument #1 ($exponent) exponent is too large"),
        BcError::PowModNegativeExponent => format!("BcMath\\Number::{method}(): Argument #1 ($exponent) must be greater than or equal to 0"),
        BcError::PrecisionRange => format!("BcMath\\Number::{method}(): Argument #1 ($precision) must be between -9223372036854775808 and 2147483647"),
        _ => error.php_message(),
    };
    BcError::Number { status: error.status_code(), message }
}

#[cfg(test)]
mod tests;
