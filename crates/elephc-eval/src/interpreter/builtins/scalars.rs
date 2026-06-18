//! Purpose:
//! Scalar conversion, math helpers, type predicates, and trailing string-search builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime
//!   behavior through `RuntimeValueOps`.

use super::super::*;
use super::*;

/// Returns the standard zlib/PHP CRC-32 checksum for a byte slice.
pub(in crate::interpreter) fn eval_crc32_bytes(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// Casts one eval value to PHP int and returns the scalar payload.
pub(in crate::interpreter) fn eval_int_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let value = values.cast_int(value)?;
    let bytes = values.string_bytes(value)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .parse::<i64>()
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP's `bin2hex(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_bin2hex(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_bin2hex_result(value, values)
}

/// Converts one eval value through PHP string conversion and returns lowercase hex bytes.
pub(in crate::interpreter) fn eval_bin2hex_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.string(&eval_lower_hex_bytes(&bytes))
}

/// Converts bytes to lowercase hexadecimal text.
pub(in crate::interpreter) fn eval_lower_hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

/// Evaluates PHP's `hex2bin(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_hex2bin(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_hex2bin_result(value, values)
}

/// Converts one eval value through PHP string conversion and decodes hexadecimal bytes.
pub(in crate::interpreter) fn eval_hex2bin_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    if bytes.len() % 2 != 0 {
        values.warning(HEX2BIN_ODD_LENGTH_WARNING)?;
        return values.bool_value(false);
    }
    let mut output = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let Some(high) = eval_hex_nibble(pair[0]) else {
            values.warning(HEX2BIN_INVALID_WARNING)?;
            return values.bool_value(false);
        };
        let Some(low) = eval_hex_nibble(pair[1]) else {
            values.warning(HEX2BIN_INVALID_WARNING)?;
            return values.bool_value(false);
        };
        output.push((high << 4) | low);
    }
    values.string_bytes_value(&output)
}

/// Returns the four-bit value for one hexadecimal byte.
pub(in crate::interpreter) fn eval_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Evaluates PHP's `addslashes(...)` or `stripslashes(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_slashes(
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
    eval_slashes_result(name, value, values)
}

/// Applies PHP byte-string escaping or unescaping for addslashes/stripslashes.
pub(in crate::interpreter) fn eval_slashes_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "addslashes" => eval_addslashes_result(value, values),
        "stripslashes" => eval_stripslashes_result(value, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Escapes NUL, quotes, and backslashes using PHP `addslashes()` byte semantics.
pub(in crate::interpreter) fn eval_addslashes_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    for byte in bytes {
        match byte {
            0 => output.extend_from_slice(b"\\0"),
            b'\'' | b'"' | b'\\' => {
                output.push(b'\\');
                output.push(byte);
            }
            _ => output.push(byte),
        }
    }
    values.string_bytes_value(&output)
}

/// Removes backslash quoting using PHP `stripslashes()` byte semantics.
pub(in crate::interpreter) fn eval_stripslashes_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 1;
            if let Some(byte) = bytes.get(index).copied() {
                output.push(if byte == b'0' { 0 } else { byte });
                index += 1;
            }
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    values.string_bytes_value(&output)
}

/// Evaluates PHP's `base64_encode(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_base64_encode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_base64_encode_result(value, values)
}

/// Converts one eval value through PHP string conversion and returns Base64 text.
pub(in crate::interpreter) fn eval_base64_encode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = String::with_capacity(((bytes.len() + 2) / 3) * 4);
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(third & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    values.string(&output)
}

/// Evaluates PHP's one-argument `base64_decode(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_base64_decode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_base64_decode_result(value, values)
}

/// Converts one eval value through PHP string conversion and decodes Base64 bytes.
pub(in crate::interpreter) fn eval_base64_decode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let input = values.string_bytes(value)?;
    let mut output = Vec::with_capacity((input.len() / 4) * 3);
    let mut quartet = Vec::with_capacity(4);
    for byte in input {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            quartet.push(None);
        } else if let Some(value) = eval_base64_decode_sextet(byte) {
            quartet.push(Some(value));
        } else {
            continue;
        }
        if quartet.len() == 4 {
            eval_push_base64_decoded_quartet(&quartet, &mut output);
            quartet.clear();
        }
    }
    if !quartet.is_empty() {
        while quartet.len() < 4 {
            quartet.push(None);
        }
        eval_push_base64_decoded_quartet(&quartet, &mut output);
    }
    values.string_bytes_value(&output)
}

/// Returns the six-bit Base64 value for one encoded byte.
pub(in crate::interpreter) fn eval_base64_decode_sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Appends decoded bytes for one padded or unpadded Base64 quartet.
pub(in crate::interpreter) fn eval_push_base64_decoded_quartet(
    quartet: &[Option<u8>],
    output: &mut Vec<u8>,
) {
    let (Some(first), Some(second)) = (quartet[0], quartet[1]) else {
        return;
    };
    output.push((first << 2) | (second >> 4));
    let Some(third) = quartet[2] else {
        return;
    };
    output.push(((second & 0x0f) << 4) | (third >> 2));
    let Some(fourth) = quartet[3] else {
        return;
    };
    output.push(((third & 0x03) << 6) | fourth);
}

/// Evaluates PHP one-argument floating-point math builtins over one eval expression.
pub(in crate::interpreter) fn eval_builtin_float_unary(
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
    eval_float_unary_result(name, value, values)
}

/// Dispatches an evaluated value through the matching PHP floating-point unary math function.
pub(in crate::interpreter) fn eval_float_unary_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_float_value(value, values)?;
    let result = match name {
        "acos" => value.acos(),
        "asin" => value.asin(),
        "atan" => value.atan(),
        "cos" => value.cos(),
        "cosh" => value.cosh(),
        "deg2rad" => value.to_radians(),
        "exp" => value.exp(),
        "log2" => value.log2(),
        "log10" => value.log10(),
        "rad2deg" => value.to_degrees(),
        "sin" => value.sin(),
        "sinh" => value.sinh(),
        "tan" => value.tan(),
        "tanh" => value.tanh(),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.float(result)
}

/// Evaluates PHP two-argument floating-point math builtins over eval expressions.
pub(in crate::interpreter) fn eval_builtin_float_pair(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_float_pair_result(name, left, right, values)
}

/// Dispatches an evaluated pair through PHP `atan2()` or `hypot()`.
pub(in crate::interpreter) fn eval_float_pair_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left = eval_float_value(left, values)?;
    let right = eval_float_value(right, values)?;
    let result = match name {
        "atan2" => left.atan2(right),
        "hypot" => left.hypot(right),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.float(result)
}

/// Evaluates PHP `log($num, $base = e)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_log(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [num] => {
            let num = eval_expr(num, context, scope, values)?;
            eval_log_result(num, None, values)
        }
        [num, base] => {
            let num = eval_expr(num, context, scope, values)?;
            let base = eval_expr(base, context, scope, values)?;
            eval_log_result(num, Some(base), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Computes PHP `log()` from already evaluated arguments.
pub(in crate::interpreter) fn eval_log_result(
    num: RuntimeCellHandle,
    base: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let num = eval_float_value(num, values)?;
    let result = match base {
        Some(base) => num.log(eval_float_value(base, values)?),
        None => num.ln(),
    };
    values.float(result)
}

/// Evaluates PHP `intdiv(...)` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_intdiv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_intdiv_result(left, right, values)
}

/// Computes PHP integer division from already evaluated arguments.
pub(in crate::interpreter) fn eval_intdiv_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left = eval_int_value(left, values)?;
    let right = eval_int_value(right, values)?;
    if right == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = left.checked_div(right).ok_or(EvalStatus::RuntimeFatal)?;
    values.int(result)
}

/// Evaluates PHP floating-point binary math builtins over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_float_binary(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_float_binary_result(name, left, right, values)
}

/// Dispatches an evaluated pair through the matching PHP float math hook.
pub(in crate::interpreter) fn eval_float_binary_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "fdiv" => values.fdiv(left, right),
        "fmod" => values.fmod(left, right),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP `clamp($value, $min, $max)` over three eval expressions.
pub(in crate::interpreter) fn eval_builtin_clamp(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value, min, max] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let min = eval_expr(min, context, scope, values)?;
    let max = eval_expr(max, context, scope, values)?;
    eval_clamp_result(value, min, max, values)
}

/// Selects the inclusive clamp result after validating bound order and NaN bounds.
pub(in crate::interpreter) fn eval_clamp_result(
    value: RuntimeCellHandle,
    min: RuntimeCellHandle,
    max: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_clamp_bound_is_nan(min, values)? || eval_clamp_bound_is_nan(max, values)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let invalid_bounds = values.compare(EvalBinOp::Gt, min, max)?;
    if values.truthy(invalid_bounds)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let above_max = values.compare(EvalBinOp::Gt, value, max)?;
    if values.truthy(above_max)? {
        return Ok(max);
    }
    let below_min = values.compare(EvalBinOp::Lt, value, min)?;
    if values.truthy(below_min)? {
        return Ok(min);
    }
    Ok(value)
}

/// Returns whether a clamp bound is a floating-point NaN value.
pub(in crate::interpreter) fn eval_clamp_bound_is_nan(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_FLOAT {
        return Ok(false);
    }
    Ok(eval_float_value(value, values)?.is_nan())
}

/// Evaluates PHP numeric `min(...)` and `max(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_min_max(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_min_max_result(name, &evaluated_args, values)
}

/// Selects the smallest or largest evaluated cell using runtime comparison hooks.
pub(in crate::interpreter) fn eval_min_max_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((&first, rest)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let op = match name {
        "min" => EvalBinOp::Lt,
        "max" => EvalBinOp::Gt,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let mut selected = first;
    for candidate in rest {
        let better = values.compare(op, *candidate, selected)?;
        if values.truthy(better)? {
            selected = *candidate;
        }
    }
    Ok(selected)
}

/// Evaluates PHP scalar cast builtins over one eval expression.
pub(in crate::interpreter) fn eval_builtin_cast(
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
    eval_cast_result(name, value, values)
}

/// Dispatches an already evaluated value through the matching PHP cast hook.
pub(in crate::interpreter) fn eval_cast_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "intval" => values.cast_int(value),
        "floatval" => values.cast_float(value),
        "strval" => values.cast_string(value),
        "boolval" => values.cast_bool(value),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP's `gettype(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_gettype(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_gettype_result(value, values)
}

/// Converts one boxed runtime tag into PHP's `gettype()` spelling.
pub(in crate::interpreter) fn eval_gettype_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    values.string(eval_gettype_name(tag))
}

/// Evaluates PHP's `get_class(...)` over one eval object expression.
pub(in crate::interpreter) fn eval_builtin_get_class(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let object = eval_expr(object, context, scope, values)?;
    eval_get_class_result(object, context, values)
}

/// Resolves the PHP-visible class name for one already materialized object cell.
pub(in crate::interpreter) fn eval_get_class_result(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Ok(identity) = values.object_identity(object) {
        if let Some(class) = context.dynamic_object_class(identity) {
            return values.string(class.name().trim_start_matches('\\'));
        }
    }
    values.object_class_name(object)
}

/// Evaluates PHP's SPL object identity builtins over one eval object expression.
pub(in crate::interpreter) fn eval_builtin_spl_object_identity(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let object = eval_expr(object, context, scope, values)?;
    eval_spl_object_identity_result(name, object, values)
}

/// Returns the unboxed object-payload identity in the native SPL builtin spelling.
pub(in crate::interpreter) fn eval_spl_object_identity_result(
    name: &str,
    object: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let identity = values.object_identity(object)? as i64;
    match name {
        "spl_object_id" => values.int(identity),
        "spl_object_hash" => values.string(&identity.to_string()),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP's `get_parent_class(...)` over one eval object or class-name expression.
pub(in crate::interpreter) fn eval_builtin_get_parent_class(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object_or_class] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let object_or_class = eval_expr(object_or_class, context, scope, values)?;
    eval_get_parent_class_result(object_or_class, values)
}

/// Resolves the PHP-visible parent class name for one object or class-name cell.
pub(in crate::interpreter) fn eval_get_parent_class_result(
    object_or_class: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.parent_class_name(object_or_class)
}

/// Evaluates `get_resource_type(...)` and `get_resource_id(...)` over one eval value.
pub(in crate::interpreter) fn eval_builtin_resource_introspection(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [resource] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let resource = eval_expr(resource, context, scope, values)?;
    eval_resource_introspection_result(name, resource, values)
}

/// Evaluates a materialized resource introspection builtin argument.
pub(in crate::interpreter) fn eval_resource_introspection_result(
    name: &str,
    resource: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(resource)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    match name {
        "get_resource_type" => values.string("stream"),
        "get_resource_id" => values.cast_int(resource),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Returns the PHP-visible type name for a concrete eval runtime tag.
pub(in crate::interpreter) fn eval_gettype_name(tag: u64) -> &'static str {
    match tag {
        EVAL_TAG_INT => "integer",
        EVAL_TAG_FLOAT => "double",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_BOOL => "boolean",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_OBJECT => "object",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_NULL => "NULL",
        _ => "NULL",
    }
}

/// Evaluates PHP scalar/container type predicate builtins over one eval expression.
pub(in crate::interpreter) fn eval_builtin_type_predicate(
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
    eval_type_predicate_result(name, value, values)
}

/// Converts a concrete runtime tag into a PHP `is_*` predicate result.
pub(in crate::interpreter) fn eval_type_predicate_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    let result = match name {
        "is_int" | "is_integer" | "is_long" => tag == EVAL_TAG_INT,
        "is_float" | "is_double" | "is_real" => tag == EVAL_TAG_FLOAT,
        "is_string" => tag == EVAL_TAG_STRING,
        "is_bool" => tag == EVAL_TAG_BOOL,
        "is_null" => tag == EVAL_TAG_NULL,
        "is_array" | "is_iterable" => matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC),
        "is_object" => tag == EVAL_TAG_OBJECT,
        "is_resource" => tag == EVAL_TAG_RESOURCE,
        "is_nan" => eval_float_value(value, values)?.is_nan(),
        "is_infinite" => eval_float_value(value, values)?.is_infinite(),
        "is_finite" => eval_float_value(value, values)?.is_finite(),
        "is_numeric" => {
            tag == EVAL_TAG_INT
                || tag == EVAL_TAG_FLOAT
                || (tag == EVAL_TAG_STRING && eval_is_numeric_string(&values.string_bytes(value)?))
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.bool_value(result)
}

/// Matches the static backend's legacy ASCII numeric-string scan.
pub(in crate::interpreter) fn eval_is_numeric_string(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    let mut index = 0;
    let mut consumed_digits = 0;
    if bytes[index] == b'-' {
        index += 1;
        if index >= bytes.len() {
            return false;
        }
    }

    while index < bytes.len() {
        if bytes[index] == b'.' {
            index += 1;
            break;
        }
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    consumed_digits > 0
}

/// Evaluates PHP's `hash_equals(...)` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_hash_equals(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [known, user] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let known = eval_expr(known, context, scope, values)?;
    let user = eval_expr(user, context, scope, values)?;
    eval_hash_equals_result(known, user, values)
}

/// Compares two converted strings with PHP `hash_equals()` semantics.
pub(in crate::interpreter) fn eval_hash_equals_result(
    known: RuntimeCellHandle,
    user: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let known = values.string_bytes(known)?;
    let user = values.string_bytes(user)?;
    if known.len() != user.len() {
        return values.bool_value(false);
    }
    let mut diff = 0u8;
    for (known, user) in known.iter().zip(user.iter()) {
        diff |= known ^ user;
    }
    values.bool_value(diff == 0)
}

/// Evaluates PHP string comparison builtins over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_string_compare(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_string_compare_result(name, left, right, values)
}

/// Compares two converted strings and returns -1, 0, or 1.
pub(in crate::interpreter) fn eval_string_compare_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut left = values.string_bytes(left)?;
    let mut right = values.string_bytes(right)?;
    match name {
        "strcmp" => {}
        "strcasecmp" => {
            left.make_ascii_lowercase();
            right.make_ascii_lowercase();
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    let result = match left.cmp(&right) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    };
    values.int(result)
}

/// Evaluates PHP's byte-string search predicates over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_string_search(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [haystack, needle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let haystack = eval_expr(haystack, context, scope, values)?;
    let needle = eval_expr(needle, context, scope, values)?;
    eval_string_search_result(name, haystack, needle, values)
}

/// Checks one converted haystack for one converted needle using PHP byte-string semantics.
pub(in crate::interpreter) fn eval_string_search_result(
    name: &str,
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let matched = match name {
        "str_contains" => {
            needle.is_empty()
                || haystack
                    .windows(needle.len())
                    .any(|window| window == needle)
        }
        "str_starts_with" => haystack.starts_with(&needle),
        "str_ends_with" => haystack.ends_with(&needle),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.bool_value(matched)
}

/// Evaluates PHP byte-string position builtins over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_string_position(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [haystack, needle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let haystack = eval_expr(haystack, context, scope, values)?;
    let needle = eval_expr(needle, context, scope, values)?;
    eval_string_position_result(name, haystack, needle, values)
}

/// Returns the first or last byte offset of a converted needle, or PHP `false`.
pub(in crate::interpreter) fn eval_string_position_result(
    name: &str,
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let position = match name {
        "strpos" if needle.is_empty() => Some(0),
        "strpos" => haystack
            .windows(needle.len())
            .position(|window| window == needle),
        "strrpos" if needle.is_empty() => Some(haystack.len()),
        "strrpos" => haystack
            .windows(needle.len())
            .rposition(|window| window == needle),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    match position {
        Some(position) => {
            let position = i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(position)
        }
        None => values.bool_value(false),
    }
}

/// Evaluates PHP `strstr(...)` over haystack, needle, and optional prefix mode.
pub(in crate::interpreter) fn eval_builtin_strstr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [haystack, needle] => {
            let haystack = eval_expr(haystack, context, scope, values)?;
            let needle = eval_expr(needle, context, scope, values)?;
            eval_strstr_result(haystack, needle, false, values)
        }
        [haystack, needle, before_needle] => {
            let haystack = eval_expr(haystack, context, scope, values)?;
            let needle = eval_expr(needle, context, scope, values)?;
            let before_needle = eval_expr(before_needle, context, scope, values)?;
            let before_needle = values.truthy(before_needle)?;
            eval_strstr_result(haystack, needle, before_needle, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the suffix or prefix selected by PHP `strstr()`, or `false` when absent.
pub(in crate::interpreter) fn eval_strstr_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    before_needle: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let position = if needle.is_empty() {
        Some(0)
    } else {
        eval_find_subslice(&haystack, &needle, 0)
    };
    let Some(position) = position else {
        return values.bool_value(false);
    };
    let result = if before_needle {
        &haystack[..position]
    } else {
        &haystack[position..]
    };
    values.string_bytes_value(result)
}

pub(in crate::interpreter) const PHP_DEFAULT_TRIM_MASK: &[u8] = b" \n\r\t\x0B\x0C\0";

/// Evaluates PHP trim-like string builtins over one eval expression and optional mask.
pub(in crate::interpreter) fn eval_builtin_trim_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_trim_like_result(name, value, None, values)
        }
        [value, mask] => {
            let value = eval_expr(value, context, scope, values)?;
            let mask = eval_expr(mask, context, scope, values)?;
            eval_trim_like_result(name, value, Some(mask), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Trims one converted string using PHP's default mask or a caller-provided byte mask.
pub(in crate::interpreter) fn eval_trim_like_result(
    name: &str,
    value: RuntimeCellHandle,
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let explicit_mask;
    let trim_mask = if let Some(mask) = mask {
        explicit_mask = values.string_bytes(mask)?;
        explicit_mask.as_slice()
    } else {
        PHP_DEFAULT_TRIM_MASK
    };

    let mut start = 0;
    let mut end = bytes.len();
    if matches!(name, "trim" | "ltrim") {
        while start < end && trim_mask.contains(&bytes[start]) {
            start += 1;
        }
    }
    if matches!(name, "trim" | "rtrim" | "chop") {
        while end > start && trim_mask.contains(&bytes[end - 1]) {
            end -= 1;
        }
    }
    if !matches!(name, "trim" | "ltrim" | "rtrim" | "chop") {
        return Err(EvalStatus::UnsupportedConstruct);
    }

    let value =
        String::from_utf8(bytes[start..end].to_vec()).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}

/// Evaluates PHP ASCII case-conversion string builtins over one eval expression.
pub(in crate::interpreter) fn eval_builtin_string_case(
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
    eval_string_case_result(name, value, values)
}

/// Converts one eval value through PHP string conversion and ASCII case mapping.
pub(in crate::interpreter) fn eval_string_case_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    match name {
        "strtolower" => {
            for byte in &mut bytes {
                if byte.is_ascii_uppercase() {
                    *byte += b'a' - b'A';
                }
            }
        }
        "strtoupper" => {
            for byte in &mut bytes {
                if byte.is_ascii_lowercase() {
                    *byte -= b'a' - b'A';
                }
            }
        }
        "ucfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_lowercase()) {
                bytes[0] -= b'a' - b'A';
            }
        }
        "lcfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_uppercase()) {
                bytes[0] += b'a' - b'A';
            }
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    let value = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}

/// Evaluates PHP `ucwords(...)` over one string and optional separator expression.
pub(in crate::interpreter) fn eval_builtin_ucwords(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_ucwords_result(value, None, values)
        }
        [value, separators] => {
            let value = eval_expr(value, context, scope, values)?;
            let separators = eval_expr(separators, context, scope, values)?;
            eval_ucwords_result(value, Some(separators), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Uppercases ASCII lowercase bytes at the start of words separated by PHP delimiters.
pub(in crate::interpreter) fn eval_ucwords_result(
    value: RuntimeCellHandle,
    separators: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    let separators = match separators {
        Some(separators) => values.string_bytes(separators)?,
        None => b" \t\r\n\x0c\x0b".to_vec(),
    };
    let mut word_start = true;
    for byte in &mut bytes {
        if separators.contains(byte) {
            word_start = true;
        } else if word_start {
            if byte.is_ascii_lowercase() {
                *byte -= b'a' - b'A';
            }
            word_start = false;
        }
    }
    values.string_bytes_value(&bytes)
}

/// Evaluates PHP `wordwrap(...)` over one string and optional wrapping controls.
pub(in crate::interpreter) fn eval_builtin_wordwrap(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_wordwrap_result(value, None, None, None, values)
        }
        [value, width] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), None, None, values)
        }
        [value, width, break_string] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            let break_string = eval_expr(break_string, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), Some(break_string), None, values)
        }
        [value, width, break_string, cut] => {
            let value = eval_expr(value, context, scope, values)?;
            let width = eval_expr(width, context, scope, values)?;
            let break_string = eval_expr(break_string, context, scope, values)?;
            let cut = eval_expr(cut, context, scope, values)?;
            eval_wordwrap_result(value, Some(width), Some(break_string), Some(cut), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Wraps a byte string at PHP word boundaries and preserves existing newlines.
pub(in crate::interpreter) fn eval_wordwrap_result(
    value: RuntimeCellHandle,
    width: Option<RuntimeCellHandle>,
    break_string: Option<RuntimeCellHandle>,
    cut: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let width = match width {
        Some(width) => eval_int_value(width, values)?,
        None => 75,
    };
    let break_string = match break_string {
        Some(break_string) => values.string_bytes(break_string)?,
        None => b"\n".to_vec(),
    };
    if break_string.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let cut = match cut {
        Some(cut) => values.truthy(cut)?,
        None => false,
    };
    if width == 0 && cut {
        return Err(EvalStatus::RuntimeFatal);
    }
    if bytes.is_empty() {
        return values.string_bytes_value(&bytes);
    }
    let output = eval_wordwrap_bytes(&bytes, width, &break_string, cut);
    values.string_bytes_value(&output)
}

/// Applies the core PHP word-wrap scan over already converted byte slices.
pub(in crate::interpreter) fn eval_wordwrap_bytes(
    bytes: &[u8],
    width: i64,
    break_string: &[u8],
    cut: bool,
) -> Vec<u8> {
    if width < 0 && cut {
        let mut output = Vec::with_capacity(bytes.len() + (bytes.len() * break_string.len()));
        for byte in bytes {
            output.extend_from_slice(break_string);
            output.push(*byte);
        }
        return output;
    }

    let width = width.max(0) as usize;
    let mut output = Vec::with_capacity(bytes.len());
    let mut line_start = 0;
    let mut last_space = None;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                output.extend_from_slice(&bytes[line_start..=index]);
                index += 1;
                line_start = index;
                last_space = None;
            }
            b' ' => {
                if index.saturating_sub(line_start) >= width {
                    output.extend_from_slice(&bytes[line_start..index]);
                    output.extend_from_slice(break_string);
                    index += 1;
                    line_start = index;
                    last_space = None;
                } else {
                    last_space = Some(index);
                    index += 1;
                }
            }
            _ if index.saturating_sub(line_start) >= width => {
                if let Some(space) = last_space {
                    output.extend_from_slice(&bytes[line_start..space]);
                    output.extend_from_slice(break_string);
                    line_start = space + 1;
                    last_space = None;
                } else if cut && width > 0 {
                    output.extend_from_slice(&bytes[line_start..index]);
                    output.extend_from_slice(break_string);
                    line_start = index;
                } else {
                    index += 1;
                }
            }
            _ => {
                index += 1;
            }
        }
    }
    output.extend_from_slice(&bytes[line_start..]);
    output
}
