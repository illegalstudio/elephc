//! Purpose:
//! Dispatch hook enums for eval builtins that have migrated into declarative
//! registry entries.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` when a migrated builtin is invoked.
//!
//! Key details:
//! - Hooks keep the registry static while calls remain generic over
//!   `RuntimeValueOps`.
//! - Existing direct and evaluated-result helpers still own PHP semantics.

use super::super::{
    eval_builtin_base64_decode, eval_builtin_base64_encode, eval_builtin_bin2hex,
    eval_builtin_ceil, eval_builtin_chr, eval_builtin_clamp, eval_builtin_count,
    eval_builtin_crc32, eval_builtin_ctype, eval_builtin_float_binary, eval_builtin_float_pair,
    eval_builtin_float_unary, eval_builtin_floor, eval_builtin_gettype, eval_builtin_hex2bin,
    eval_builtin_intdiv, eval_builtin_log, eval_builtin_min_max, eval_builtin_number_format,
    eval_builtin_ord, eval_builtin_pi, eval_builtin_pow, eval_builtin_round, eval_builtin_slashes,
    eval_builtin_sqrt, eval_builtin_str_repeat, eval_builtin_strlen, eval_builtin_type_predicate,
    eval_builtin_url_decode, eval_builtin_url_encode, eval_count_result, eval_ord_result,
    ElephcEvalContext, ElephcEvalScope, EvalExpr, EvalStatus, RuntimeCellHandle, RuntimeValueOps,
};
use super::{
    eval_base64_decode_result, eval_base64_encode_result, eval_bin2hex_result, eval_builtin_abs,
    eval_builtin_cast, eval_builtin_strrev, eval_cast_result, eval_chr_result, eval_clamp_result,
    eval_crc32_result, eval_ctype_result, eval_float_binary_result, eval_float_pair_result,
    eval_float_unary_result, eval_gettype_result, eval_hex2bin_result, eval_intdiv_result,
    eval_log_result, eval_min_max_result, eval_number_format_result, eval_slashes_result,
    eval_str_repeat_result, eval_type_predicate_result, eval_url_decode_result,
    eval_url_encode_result,
};

/// Direct expression-level dispatch hooks for migrated builtins.
#[derive(Clone, Copy)]
pub(in crate::interpreter) enum EvalDirectHook {
    /// Dispatches `abs(...)`.
    Abs,
    /// Dispatches `base64_decode(...)`.
    Base64Decode,
    /// Dispatches `base64_encode(...)`.
    Base64Encode,
    /// Dispatches `bin2hex(...)`.
    Bin2Hex,
    /// Dispatches scalar cast builtins.
    Cast,
    /// Dispatches `ceil(...)`.
    Ceil,
    /// Dispatches `chr(...)`.
    Chr,
    /// Dispatches `clamp(...)`.
    Clamp,
    /// Dispatches `count(...)`.
    Count,
    /// Dispatches `crc32(...)`.
    Crc32,
    /// Dispatches `ctype_*` predicates.
    Ctype,
    /// Dispatches binary floating-point builtins.
    FloatBinary,
    /// Dispatches paired floating-point builtins.
    FloatPair,
    /// Dispatches unary floating-point builtins.
    FloatUnary,
    /// Dispatches `floor(...)`.
    Floor,
    /// Dispatches `gettype(...)`.
    Gettype,
    /// Dispatches `hex2bin(...)`.
    Hex2Bin,
    /// Dispatches `intdiv(...)`.
    Intdiv,
    /// Dispatches `log(...)`.
    Log,
    /// Dispatches `min(...)` and `max(...)`.
    MinMax,
    /// Dispatches `number_format(...)`.
    NumberFormat,
    /// Dispatches `ord(...)`.
    Ord,
    /// Dispatches `pi()`.
    Pi,
    /// Dispatches `pow(...)`.
    Pow,
    /// Dispatches `round(...)`.
    Round,
    /// Dispatches `addslashes(...)` and `stripslashes(...)`.
    Slashes,
    /// Dispatches `sqrt(...)`.
    Sqrt,
    /// Dispatches `strlen(...)`.
    Strlen,
    /// Dispatches `str_repeat(...)`.
    StrRepeat,
    /// Dispatches `strrev(...)`.
    Strrev,
    /// Dispatches scalar and container type predicates.
    TypePredicate,
    /// Dispatches URL decode builtins.
    UrlDecode,
    /// Dispatches URL encode builtins.
    UrlEncode,
}

/// Evaluated-argument dispatch hooks for migrated builtins.
#[derive(Clone, Copy)]
pub(in crate::interpreter) enum EvalValuesHook {
    /// Dispatches `abs(...)`.
    Abs,
    /// Dispatches `base64_decode(...)`.
    Base64Decode,
    /// Dispatches `base64_encode(...)`.
    Base64Encode,
    /// Dispatches `bin2hex(...)`.
    Bin2Hex,
    /// Dispatches scalar cast builtins.
    Cast,
    /// Dispatches `ceil(...)`.
    Ceil,
    /// Dispatches `chr(...)`.
    Chr,
    /// Dispatches `clamp(...)`.
    Clamp,
    /// Dispatches `count(...)`.
    Count,
    /// Dispatches `crc32(...)`.
    Crc32,
    /// Dispatches `ctype_*` predicates.
    Ctype,
    /// Dispatches binary floating-point builtins.
    FloatBinary,
    /// Dispatches paired floating-point builtins.
    FloatPair,
    /// Dispatches unary floating-point builtins.
    FloatUnary,
    /// Dispatches `floor(...)`.
    Floor,
    /// Dispatches `gettype(...)`.
    Gettype,
    /// Dispatches `hex2bin(...)`.
    Hex2Bin,
    /// Dispatches `intdiv(...)`.
    Intdiv,
    /// Dispatches `log(...)`.
    Log,
    /// Dispatches `min(...)` and `max(...)`.
    MinMax,
    /// Dispatches `number_format(...)`.
    NumberFormat,
    /// Dispatches `ord(...)`.
    Ord,
    /// Dispatches `pi()`.
    Pi,
    /// Dispatches `pow(...)`.
    Pow,
    /// Dispatches `round(...)`.
    Round,
    /// Dispatches `addslashes(...)` and `stripslashes(...)`.
    Slashes,
    /// Dispatches `sqrt(...)`.
    Sqrt,
    /// Dispatches `strlen(...)`.
    Strlen,
    /// Dispatches `str_repeat(...)`.
    StrRepeat,
    /// Dispatches `strrev(...)`.
    Strrev,
    /// Dispatches scalar and container type predicates.
    TypePredicate,
    /// Dispatches URL decode builtins.
    UrlDecode,
    /// Dispatches URL encode builtins.
    UrlEncode,
}

impl EvalDirectHook {
    /// Runs a direct expression-level builtin call through the migrated hook.
    pub(in crate::interpreter) fn call(
        self,
        name: &str,
        args: &[EvalExpr],
        context: &mut ElephcEvalContext,
        scope: &mut ElephcEvalScope,
        values: &mut impl RuntimeValueOps,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self {
            Self::Abs => eval_builtin_abs(args, context, scope, values),
            Self::Base64Decode => eval_builtin_base64_decode(args, context, scope, values),
            Self::Base64Encode => eval_builtin_base64_encode(args, context, scope, values),
            Self::Bin2Hex => eval_builtin_bin2hex(args, context, scope, values),
            Self::Cast => eval_builtin_cast(name, args, context, scope, values),
            Self::Ceil => eval_builtin_ceil(args, context, scope, values),
            Self::Chr => eval_builtin_chr(args, context, scope, values),
            Self::Clamp => eval_builtin_clamp(args, context, scope, values),
            Self::Count => eval_builtin_count(args, context, scope, values),
            Self::Crc32 => eval_builtin_crc32(args, context, scope, values),
            Self::Ctype => eval_builtin_ctype(name, args, context, scope, values),
            Self::FloatBinary => eval_builtin_float_binary(name, args, context, scope, values),
            Self::FloatPair => eval_builtin_float_pair(name, args, context, scope, values),
            Self::FloatUnary => eval_builtin_float_unary(name, args, context, scope, values),
            Self::Floor => eval_builtin_floor(args, context, scope, values),
            Self::Gettype => eval_builtin_gettype(args, context, scope, values),
            Self::Hex2Bin => eval_builtin_hex2bin(args, context, scope, values),
            Self::Intdiv => eval_builtin_intdiv(args, context, scope, values),
            Self::Log => eval_builtin_log(args, context, scope, values),
            Self::MinMax => eval_builtin_min_max(name, args, context, scope, values),
            Self::NumberFormat => eval_builtin_number_format(args, context, scope, values),
            Self::Ord => eval_builtin_ord(args, context, scope, values),
            Self::Pi => eval_builtin_pi(args, values),
            Self::Pow => eval_builtin_pow(args, context, scope, values),
            Self::Round => eval_builtin_round(args, context, scope, values),
            Self::Slashes => eval_builtin_slashes(name, args, context, scope, values),
            Self::Sqrt => eval_builtin_sqrt(args, context, scope, values),
            Self::Strlen => eval_builtin_strlen(args, context, scope, values),
            Self::StrRepeat => eval_builtin_str_repeat(args, context, scope, values),
            Self::Strrev => eval_builtin_strrev(args, context, scope, values),
            Self::TypePredicate => eval_builtin_type_predicate(name, args, context, scope, values),
            Self::UrlDecode => eval_builtin_url_decode(name, args, context, scope, values),
            Self::UrlEncode => eval_builtin_url_encode(name, args, context, scope, values),
        }
    }
}

impl EvalValuesHook {
    /// Runs an evaluated-argument builtin call through the migrated hook.
    pub(in crate::interpreter) fn call(
        self,
        name: &str,
        evaluated_args: &[RuntimeCellHandle],
        context: &mut ElephcEvalContext,
        values: &mut impl RuntimeValueOps,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self {
            Self::Abs => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                values.abs(*value)
            }
            Self::Base64Decode => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_base64_decode_result(*value, values)
            }
            Self::Base64Encode => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_base64_encode_result(*value, values)
            }
            Self::Bin2Hex => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_bin2hex_result(*value, values)
            }
            Self::Cast => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_cast_result(name, *value, context, values)
            }
            Self::Ceil => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                values.ceil(*value)
            }
            Self::Chr => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_chr_result(*value, values)
            }
            Self::Clamp => {
                let [value, min, max] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_clamp_result(*value, *min, *max, values)
            }
            Self::Count => match evaluated_args {
                [value] => eval_count_result(*value, None, context, values),
                [value, mode] => eval_count_result(*value, Some(*mode), context, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Crc32 => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_crc32_result(*value, values)
            }
            Self::Ctype => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_ctype_result(name, *value, values)
            }
            Self::FloatBinary => {
                let [left, right] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_float_binary_result(name, *left, *right, values)
            }
            Self::FloatPair => {
                let [left, right] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_float_pair_result(name, *left, *right, values)
            }
            Self::FloatUnary => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_float_unary_result(name, *value, values)
            }
            Self::Floor => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                values.floor(*value)
            }
            Self::Gettype => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_gettype_result(*value, values)
            }
            Self::Hex2Bin => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_hex2bin_result(*value, values)
            }
            Self::Intdiv => {
                let [left, right] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_intdiv_result(*left, *right, values)
            }
            Self::Log => match evaluated_args {
                [num] => eval_log_result(*num, None, values),
                [num, base] => eval_log_result(*num, Some(*base), values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::MinMax => eval_min_max_result(name, evaluated_args, values),
            Self::NumberFormat => match evaluated_args {
                [value] => eval_number_format_result(*value, None, None, None, values),
                [value, decimals] => {
                    eval_number_format_result(*value, Some(*decimals), None, None, values)
                }
                [value, decimals, decimal_separator] => eval_number_format_result(
                    *value,
                    Some(*decimals),
                    Some(*decimal_separator),
                    None,
                    values,
                ),
                [value, decimals, decimal_separator, thousands_separator] => {
                    eval_number_format_result(
                        *value,
                        Some(*decimals),
                        Some(*decimal_separator),
                        Some(*thousands_separator),
                        values,
                    )
                }
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Ord => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_ord_result(*value, values)
            }
            Self::Pi => {
                if !evaluated_args.is_empty() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                values.float(std::f64::consts::PI)
            }
            Self::Pow => {
                let [left, right] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                values.pow(*left, *right)
            }
            Self::Round => match evaluated_args {
                [value] => values.round(*value, None),
                [value, precision] => values.round(*value, Some(*precision)),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Slashes => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_slashes_result(name, *value, values)
            }
            Self::Sqrt => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                values.sqrt(*value)
            }
            Self::Strlen => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                let bytes = values.string_bytes(*value)?;
                let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
                values.int(len)
            }
            Self::StrRepeat => {
                let [value, times] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_str_repeat_result(*value, *times, values)
            }
            Self::Strrev => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                values.strrev(*value)
            }
            Self::TypePredicate => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_type_predicate_result(name, *value, context, values)
            }
            Self::UrlDecode => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_url_decode_result(name, *value, values)
            }
            Self::UrlEncode => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                eval_url_encode_result(name, *value, values)
            }
        }
    }
}
