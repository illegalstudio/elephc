//! Purpose:
//! Eval registry entry and implementation for `var_export`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The COMPILED side serves `var_export` from a conditionally injected PHP prelude
//!   (`src/var_export_prelude.rs`), injected only when the compiled program itself names the
//!   function. Interpreted code cannot reach that prelude, and a Symfony request does exactly
//!   that: the routing dumper (`CompiledUrlMatcherDumper::export`) is interpreted, so the call
//!   arrived as `call to undefined function var_export()`. The contract is
//!   `BuiltinKind::PreludeProvided` for that asymmetry -- the `levenshtein` shape.
//! - The rendering below is a TRANSCRIPTION of that prelude, which was itself measured against
//!   `php -n`. The layout details that look arbitrary are not: object properties indent by
//!   THREE spaces while array entries indent by two, a nested array or object value is preceded
//!   by a newline and the entry's own padding, `stdClass` renders `(object) array(` closed by a
//!   single paren while every other class renders `\Class::__set_state(array(` closed by two,
//!   and an enum case renders as `\Enum::CASE` with no property list at all.
//! - Floats follow `serialize_precision = -1`: the shortest decimal string that round-trips,
//!   then PHP's own decimal/scientific layout thresholds (`1.0`, `0.3333333333333333`,
//!   `1.0E+17`, `1.0E-6`). `f64::to_string()` gives the shortest digits; the exponent placement
//!   is applied here rather than taken from Rust's own `Display`, which never uses the
//!   scientific form.

use super::super::super::*;

eval_builtin! {
    contract: "var_export",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates PHP `var_export()` from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_var_export(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=2).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let value = eval_expr(&args[0], context, scope, values)?;
    let return_output = match args.get(1) {
        Some(arg) => {
            let flag = eval_expr(arg, context, scope, values)?;
            values.truthy(flag)?
        }
        None => false,
    };
    eval_var_export_value_result(value, return_output, context, values)
}

/// Evaluates already materialized `var_export()` arguments.
pub(in crate::interpreter) fn eval_var_export_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=2).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let return_output = match args.get(1) {
        Some(flag) => values.truthy(*flag)?,
        None => false,
    };
    eval_var_export_value_result(args[0], return_output, context, values)
}

/// Renders, echoes, or returns one `var_export()` output string.
fn eval_var_export_value_result(
    value: RuntimeCellHandle,
    return_output: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut output = Vec::new();
    eval_var_export_append(value, 0, context, values, &mut output)?;
    if return_output {
        return values.string_bytes_value(&output);
    }
    let rendered = values.string_bytes_value(&output)?;
    values.echo(rendered)?;
    values.null()
}

/// Appends one value in PHP `var_export()` style at the given indent.
fn eval_var_export_append(
    value: RuntimeCellHandle,
    indent: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT => {
            output.extend_from_slice(&values.string_bytes(value)?);
        }
        EVAL_TAG_FLOAT => {
            let bits = values.raw_value_word(value)?;
            output.extend_from_slice(eval_var_export_float(f64::from_bits(bits)).as_bytes());
        }
        EVAL_TAG_BOOL => {
            output.extend_from_slice(if values.truthy(value)? { b"true" } else { b"false" });
        }
        EVAL_TAG_NULL => {
            output.extend_from_slice(b"NULL");
        }
        EVAL_TAG_STRING => {
            output.push(b'\'');
            eval_var_export_append_escaped(&values.string_bytes(value)?, output);
            output.push(b'\'');
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            eval_var_export_append_array(value, indent, context, values, output)?;
        }
        EVAL_TAG_OBJECT => {
            eval_var_export_append_object(value, indent, context, values, output)?;
        }
        // php renders nothing for a value it has no parsable form for, and neither does the
        // compiled prelude this transcribes.
        _ => {}
    }
    Ok(())
}

/// Appends one array in PHP `var_export()` style.
fn eval_var_export_append_array(
    value: RuntimeCellHandle,
    indent: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    output.extend_from_slice(b"array (\n");
    let len = values.array_len(value)?;
    for position in 0..len {
        let key = values.array_iter_key(value, position)?;
        let element = values.array_get(value, key)?;
        eval_var_export_append_padding(indent + 2, output);
        if values.type_tag(key)? == EVAL_TAG_INT {
            output.extend_from_slice(&values.string_bytes(key)?);
        } else {
            output.push(b'\'');
            eval_var_export_append_escaped(&values.string_bytes(key)?, output);
            output.push(b'\'');
        }
        output.extend_from_slice(b" => ");
        eval_var_export_append_entry_value(element, indent, context, values, output)?;
        output.extend_from_slice(b",\n");
    }
    eval_var_export_append_padding(indent, output);
    output.push(b')');
    Ok(())
}

/// Appends one object in PHP `var_export()` style.
fn eval_var_export_append_object(
    value: RuntimeCellHandle,
    indent: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let identity = eval_debug_object_identity(value, values);
    let class_name = eval_debug_object_class_name(value, identity, context, values)?;
    let properties = eval_debug_object_properties(value, identity, &class_name, context, values)?;
    if context.has_enum(&class_name) {
        // An enum case renders as `\\Enum::CASE`, never as a property list. Its name is the
        // readonly `name` property php gives every case.
        output.push(b'\\');
        output.extend_from_slice(class_name.as_bytes());
        output.extend_from_slice(b"::");
        if let Some(case) = properties.iter().find(|property| property.name == "name") {
            output.extend_from_slice(&values.string_bytes(case.value)?);
        }
        return Ok(());
    }
    let close: &[u8] = if class_name == "stdClass" {
        output.extend_from_slice(b"(object) array(\n");
        b")"
    } else {
        output.push(b'\\');
        output.extend_from_slice(class_name.as_bytes());
        output.extend_from_slice(b"::__set_state(array(\n");
        b"))"
    };
    for property in properties {
        if property.name.is_empty() {
            continue;
        }
        eval_var_export_append_padding(indent + 3, output);
        output.push(b'\'');
        eval_var_export_append_escaped(property.name.as_bytes(), output);
        output.extend_from_slice(b"' => ");
        eval_var_export_append_entry_value(property.value, indent, context, values, output)?;
        output.extend_from_slice(b",\n");
    }
    eval_var_export_append_padding(indent, output);
    output.extend_from_slice(close);
    Ok(())
}

/// Appends one array element or object property value, breaking the line before a nested block.
fn eval_var_export_append_entry_value(
    value: RuntimeCellHandle,
    indent: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let tag = values.type_tag(value)?;
    // An enum case renders inline like a scalar even though it is an object.
    let nested_block = match tag {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => true,
        EVAL_TAG_OBJECT => {
            let identity = eval_debug_object_identity(value, values);
            let class_name = eval_debug_object_class_name(value, identity, context, values)?;
            !context.has_enum(&class_name)
        }
        _ => false,
    };
    if nested_block {
        output.push(b'\n');
        eval_var_export_append_padding(indent + 2, output);
    }
    eval_var_export_append(value, indent + 2, context, values, output)
}

/// Appends `count` spaces of indentation.
fn eval_var_export_append_padding(count: usize, output: &mut Vec<u8>) {
    output.extend(std::iter::repeat_n(b' ', count));
}

/// Appends one single-quoted string body with PHP's two `var_export` escapes.
fn eval_var_export_append_escaped(bytes: &[u8], output: &mut Vec<u8>) {
    for byte in bytes {
        if *byte == b'\\' || *byte == b'\'' {
            output.push(b'\\');
        }
        output.push(*byte);
    }
}

/// Renders one float the way `var_export()` does at `serialize_precision = -1`.
///
/// Transcribed from `__elephc_var_export_float` in `src/var_export_prelude.rs`: shortest
/// round-trip digits, then php's own placement -- scientific when the decimal point would sit
/// before position -3 or past position 17, and a trailing `.0` whenever the decimal form would
/// otherwise look like an integer.
pub(super) fn eval_var_export_float(value: f64) -> String {
    if value.is_nan() {
        return String::from("NAN");
    }
    if value.is_infinite() {
        return String::from(if value.is_sign_negative() { "-INF" } else { "INF" });
    }
    if value == 0.0 {
        return String::from(if value.is_sign_negative() { "-0.0" } else { "0.0" });
    }
    let negative = value.is_sign_negative();
    let magnitude = value.abs();
    // `to_string()` is the shortest round-trip decimal, but never in scientific form, so the
    // digits and the decimal exponent are recovered from it rather than from its layout.
    let rendered = magnitude.to_string();
    let (digits, decimal_point) = eval_var_export_float_digits(&rendered);
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if decimal_point < -3 || decimal_point > 17 {
        out.push(digits.as_bytes()[0] as char);
        out.push('.');
        if digits.len() > 1 {
            out.push_str(&digits[1..]);
        } else {
            out.push('0');
        }
        let exponent = decimal_point - 1;
        out.push('E');
        out.push(if exponent >= 0 { '+' } else { '-' });
        out.push_str(&exponent.abs().to_string());
    } else if decimal_point <= 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-decimal_point) as usize));
        out.push_str(&digits);
    } else if decimal_point as usize >= digits.len() {
        out.push_str(&digits);
        out.extend(std::iter::repeat_n('0', decimal_point as usize - digits.len()));
        out.push_str(".0");
    } else {
        let split = decimal_point as usize;
        out.push_str(&digits[..split]);
        out.push('.');
        out.push_str(&digits[split..]);
    }
    out
}

/// Splits one non-negative shortest decimal rendering into significant digits and decimal point.
///
/// `decimal_point` is the position of the decimal point relative to the first significant digit,
/// the same `decpt` php's own conversion produces: `1.5` gives `("15", 1)` and `0.001` gives
/// `("1", -2)`.
fn eval_var_export_float_digits(rendered: &str) -> (String, i32) {
    let (integer_part, fraction_part) = match rendered.split_once('.') {
        Some((integer_part, fraction_part)) => (integer_part, fraction_part),
        None => (rendered, ""),
    };
    let mut digits = String::with_capacity(integer_part.len() + fraction_part.len());
    digits.push_str(integer_part);
    digits.push_str(fraction_part);
    let leading_zeros = digits.bytes().take_while(|byte| *byte == b'0').count();
    let trimmed = digits[leading_zeros..].trim_end_matches('0');
    let digits = if trimmed.is_empty() { "0" } else { trimmed };
    let decimal_point = if integer_part == "0" || integer_part.is_empty() {
        -(leading_zeros as i32 - integer_part.len() as i32)
    } else {
        integer_part.len() as i32
    };
    (digits.to_string(), decimal_point)
}
