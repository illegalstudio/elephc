//! Purpose:
//! Already-evaluated argument dispatch hooks for eval builtins migrated into the
//! declarative registry.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::eval_declared_builtin_values_call`.
//!
//! Key details:
//! - Values hooks run after named/default argument binding has produced PHP
//!   parameter order.
//! - Runtime-cell coercions stay in the existing builtin result helpers.

use super::super::super::{
    eval_count_result, eval_ord_result, ElephcEvalContext, EvalStatus, RuntimeCellHandle,
    RuntimeValueOps,
};
use super::super::{
    eval_array_aggregate_result, eval_array_flip_result, eval_array_pad_result,
    eval_array_projection_result, eval_array_rand_result, eval_array_reverse_result,
    eval_array_search_result, eval_array_slice_result, eval_array_unique_result,
    eval_base64_decode_result, eval_base64_encode_result, eval_bin2hex_result, eval_cast_result,
    eval_chr_result, eval_clamp_result, eval_crc32_result, eval_ctype_result,
    eval_float_binary_result, eval_float_pair_result, eval_float_unary_result,
    eval_gettype_result, eval_grapheme_strrev_result, eval_hash_equals_result,
    eval_hex2bin_result, eval_html_entity_result, eval_intdiv_result, eval_log_result,
    eval_min_max_result, eval_nl2br_result, eval_number_format_result, eval_range_result,
    eval_slashes_result, eval_str_pad_result, eval_str_replace_result, eval_str_repeat_result,
    eval_str_split_result, eval_string_case_result, eval_string_compare_result,
    eval_string_position_result, eval_string_search_result, eval_strstr_result,
    eval_substr_replace_result, eval_substr_result, eval_trim_like_result,
    eval_type_predicate_result, eval_ucwords_result, eval_url_decode_result,
    eval_url_encode_result, eval_wordwrap_result,
};

/// Evaluated-argument dispatch hooks for migrated builtins.
#[derive(Clone, Copy)]
pub(in crate::interpreter) enum EvalValuesHook {
    /// Dispatches `abs(...)`.
    Abs,
    /// Dispatches `array_sum(...)` and `array_product(...)`.
    ArrayAggregate,
    /// Dispatches `array_flip(...)`.
    ArrayFlip,
    /// Dispatches `array_key_exists(...)`.
    ArrayKeyExists,
    /// Dispatches `array_pad(...)`.
    ArrayPad,
    /// Dispatches `array_keys(...)` and `array_values(...)`.
    ArrayProjection,
    /// Dispatches `array_rand(...)`.
    ArrayRand,
    /// Dispatches `array_reverse(...)`.
    ArrayReverse,
    /// Dispatches `array_search(...)` and `in_array(...)`.
    ArraySearch,
    /// Dispatches `array_slice(...)`.
    ArraySlice,
    /// Dispatches `array_unique(...)`.
    ArrayUnique,
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
    /// Dispatches `grapheme_strrev(...)`.
    GraphemeStrrev,
    /// Dispatches `hash_equals(...)`.
    HashEquals,
    /// Dispatches `hex2bin(...)`.
    Hex2Bin,
    /// Dispatches HTML entity encode/decode builtins.
    HtmlEntity,
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
    /// Dispatches `range(...)`.
    Range,
    /// Dispatches `addslashes(...)` and `stripslashes(...)`.
    Slashes,
    /// Dispatches `sqrt(...)`.
    Sqrt,
    /// Dispatches string ASCII case-conversion builtins.
    StringCase,
    /// Dispatches string comparison builtins.
    StringCompare,
    /// Dispatches string position builtins.
    StringPosition,
    /// Dispatches string search predicate builtins.
    StringSearch,
    /// Dispatches `str_pad(...)`.
    StrPad,
    /// Dispatches `str_replace(...)` and `str_ireplace(...)`.
    StrReplace,
    /// Dispatches `str_split(...)`.
    StrSplit,
    /// Dispatches `strlen(...)`.
    Strlen,
    /// Dispatches `str_repeat(...)`.
    StrRepeat,
    /// Dispatches `strrev(...)`.
    Strrev,
    /// Dispatches `strstr(...)`.
    Strstr,
    /// Dispatches `substr(...)`.
    Substr,
    /// Dispatches `substr_replace(...)`.
    SubstrReplace,
    /// Dispatches trim-family builtins.
    TrimLike,
    /// Dispatches scalar and container type predicates.
    TypePredicate,
    /// Dispatches `ucwords(...)`.
    Ucwords,
    /// Dispatches `nl2br(...)`.
    Nl2br,
    /// Dispatches `wordwrap(...)`.
    Wordwrap,
    /// Dispatches URL decode builtins.
    UrlDecode,
    /// Dispatches URL encode builtins.
    UrlEncode,
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
            Self::Abs => one_arg(evaluated_args, values, |value, values| values.abs(value)),
            Self::ArrayAggregate => one_arg(evaluated_args, values, |array, values| {
                eval_array_aggregate_result(name, array, values)
            }),
            Self::ArrayFlip => one_arg(evaluated_args, values, eval_array_flip_result),
            Self::ArrayKeyExists => two_args(evaluated_args, values, |key, array, values| {
                values.array_key_exists(key, array)
            }),
            Self::ArrayPad => three_args(evaluated_args, values, eval_array_pad_result),
            Self::ArrayProjection => one_arg(evaluated_args, values, |array, values| {
                eval_array_projection_result(name, array, values)
            }),
            Self::ArrayRand => one_arg(evaluated_args, values, eval_array_rand_result),
            Self::ArrayReverse => match evaluated_args {
                [array] => eval_array_reverse_result(*array, false, values),
                [array, preserve_keys] => {
                    let preserve_keys = values.truthy(*preserve_keys)?;
                    eval_array_reverse_result(*array, preserve_keys, values)
                }
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::ArraySearch => two_args(evaluated_args, values, |needle, array, values| {
                eval_array_search_result(name, needle, array, values)
            }),
            Self::ArraySlice => match evaluated_args {
                [array, offset] => eval_array_slice_result(*array, *offset, None, values),
                [array, offset, length] => {
                    eval_array_slice_result(*array, *offset, Some(*length), values)
                }
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::ArrayUnique => one_arg(evaluated_args, values, eval_array_unique_result),
            Self::Base64Decode => one_arg(evaluated_args, values, eval_base64_decode_result),
            Self::Base64Encode => one_arg(evaluated_args, values, eval_base64_encode_result),
            Self::Bin2Hex => one_arg(evaluated_args, values, eval_bin2hex_result),
            Self::Cast => one_arg(evaluated_args, values, |value, values| {
                eval_cast_result(name, value, context, values)
            }),
            Self::Ceil => one_arg(evaluated_args, values, |value, values| values.ceil(value)),
            Self::Chr => one_arg(evaluated_args, values, eval_chr_result),
            Self::Clamp => three_args(evaluated_args, values, eval_clamp_result),
            Self::Count => match evaluated_args {
                [value] => eval_count_result(*value, None, context, values),
                [value, mode] => eval_count_result(*value, Some(*mode), context, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Crc32 => one_arg(evaluated_args, values, eval_crc32_result),
            Self::Ctype => one_arg(evaluated_args, values, |value, values| {
                eval_ctype_result(name, value, values)
            }),
            Self::FloatBinary => two_args(evaluated_args, values, |left, right, values| {
                eval_float_binary_result(name, left, right, values)
            }),
            Self::FloatPair => two_args(evaluated_args, values, |left, right, values| {
                eval_float_pair_result(name, left, right, values)
            }),
            Self::FloatUnary => one_arg(evaluated_args, values, |value, values| {
                eval_float_unary_result(name, value, values)
            }),
            Self::Floor => one_arg(evaluated_args, values, |value, values| values.floor(value)),
            Self::Gettype => one_arg(evaluated_args, values, eval_gettype_result),
            Self::GraphemeStrrev => one_arg(evaluated_args, values, eval_grapheme_strrev_result),
            Self::HashEquals => two_args(evaluated_args, values, eval_hash_equals_result),
            Self::Hex2Bin => one_arg(evaluated_args, values, eval_hex2bin_result),
            Self::HtmlEntity => one_arg(evaluated_args, values, |value, values| {
                eval_html_entity_result(name, value, values)
            }),
            Self::Intdiv => two_args(evaluated_args, values, eval_intdiv_result),
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
            Self::Ord => one_arg(evaluated_args, values, eval_ord_result),
            Self::Pi => {
                if !evaluated_args.is_empty() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                values.float(std::f64::consts::PI)
            }
            Self::Pow => two_args(evaluated_args, values, |left, right, values| {
                values.pow(left, right)
            }),
            Self::Round => match evaluated_args {
                [value] => values.round(*value, None),
                [value, precision] => values.round(*value, Some(*precision)),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Range => two_args(evaluated_args, values, eval_range_result),
            Self::Slashes => one_arg(evaluated_args, values, |value, values| {
                eval_slashes_result(name, value, values)
            }),
            Self::Sqrt => one_arg(evaluated_args, values, |value, values| values.sqrt(value)),
            Self::StringCase => one_arg(evaluated_args, values, |value, values| {
                eval_string_case_result(name, value, values)
            }),
            Self::StringCompare => two_args(evaluated_args, values, |left, right, values| {
                eval_string_compare_result(name, left, right, values)
            }),
            Self::StringPosition => two_args(evaluated_args, values, |haystack, needle, values| {
                eval_string_position_result(name, haystack, needle, values)
            }),
            Self::StringSearch => two_args(evaluated_args, values, |haystack, needle, values| {
                eval_string_search_result(name, haystack, needle, values)
            }),
            Self::StrPad => match evaluated_args {
                [value, length] => eval_str_pad_result(*value, *length, None, None, values),
                [value, length, pad_string] => {
                    eval_str_pad_result(*value, *length, Some(*pad_string), None, values)
                }
                [value, length, pad_string, pad_type] => {
                    eval_str_pad_result(*value, *length, Some(*pad_string), Some(*pad_type), values)
                }
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StrReplace => three_args(evaluated_args, values, |search, replace, subject, values| {
                eval_str_replace_result(name, search, replace, subject, values)
            }),
            Self::StrSplit => match evaluated_args {
                [value] => eval_str_split_result(*value, None, values),
                [value, length] => eval_str_split_result(*value, Some(*length), values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Strlen => {
                let [value] = evaluated_args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                let bytes = values.string_bytes(*value)?;
                let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
                values.int(len)
            }
            Self::StrRepeat => two_args(evaluated_args, values, eval_str_repeat_result),
            Self::Strrev => one_arg(evaluated_args, values, |value, values| values.strrev(value)),
            Self::Strstr => match evaluated_args {
                [haystack, needle] => eval_strstr_result(*haystack, *needle, false, values),
                [haystack, needle, before_needle] => {
                    let before_needle = values.truthy(*before_needle)?;
                    eval_strstr_result(*haystack, *needle, before_needle, values)
                }
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Substr => match evaluated_args {
                [value, offset] => eval_substr_result(*value, *offset, None, values),
                [value, offset, length] => {
                    eval_substr_result(*value, *offset, Some(*length), values)
                }
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::SubstrReplace => match evaluated_args {
                [value, replace, offset] => {
                    eval_substr_replace_result(*value, *replace, *offset, None, values)
                }
                [value, replace, offset, length] => {
                    eval_substr_replace_result(*value, *replace, *offset, Some(*length), values)
                }
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::TrimLike => match evaluated_args {
                [value] => eval_trim_like_result(name, *value, None, values),
                [value, mask] => eval_trim_like_result(name, *value, Some(*mask), values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::TypePredicate => one_arg(evaluated_args, values, |value, values| {
                eval_type_predicate_result(name, value, context, values)
            }),
            Self::Ucwords => match evaluated_args {
                [value] => eval_ucwords_result(*value, None, values),
                [value, separators] => eval_ucwords_result(*value, Some(*separators), values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Nl2br => match evaluated_args {
                [value] => eval_nl2br_result(*value, true, values),
                [value, use_xhtml] => {
                    let use_xhtml = values.truthy(*use_xhtml)?;
                    eval_nl2br_result(*value, use_xhtml, values)
                }
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Wordwrap => match evaluated_args {
                [value] => eval_wordwrap_result(*value, None, None, None, values),
                [value, width] => eval_wordwrap_result(*value, Some(*width), None, None, values),
                [value, width, break_string] => {
                    eval_wordwrap_result(*value, Some(*width), Some(*break_string), None, values)
                }
                [value, width, break_string, cut] => eval_wordwrap_result(
                    *value,
                    Some(*width),
                    Some(*break_string),
                    Some(*cut),
                    values,
                ),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::UrlDecode => one_arg(evaluated_args, values, |value, values| {
                eval_url_decode_result(name, value, values)
            }),
            Self::UrlEncode => one_arg(evaluated_args, values, |value, values| {
                eval_url_encode_result(name, value, values)
            }),
        }
    }
}

/// Validates and dispatches one evaluated builtin argument.
fn one_arg<V, F>(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut V,
    callback: F,
) -> Result<RuntimeCellHandle, EvalStatus>
where
    V: RuntimeValueOps,
    F: FnOnce(RuntimeCellHandle, &mut V) -> Result<RuntimeCellHandle, EvalStatus>,
{
    let [value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    callback(*value, values)
}

/// Validates and dispatches two evaluated builtin arguments.
fn two_args<V, F>(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut V,
    callback: F,
) -> Result<RuntimeCellHandle, EvalStatus>
where
    V: RuntimeValueOps,
    F: FnOnce(RuntimeCellHandle, RuntimeCellHandle, &mut V) -> Result<RuntimeCellHandle, EvalStatus>,
{
    let [left, right] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    callback(*left, *right, values)
}

/// Validates and dispatches three evaluated builtin arguments.
fn three_args<V, F>(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut V,
    callback: F,
) -> Result<RuntimeCellHandle, EvalStatus>
where
    V: RuntimeValueOps,
    F: FnOnce(
        RuntimeCellHandle,
        RuntimeCellHandle,
        RuntimeCellHandle,
        &mut V,
    ) -> Result<RuntimeCellHandle, EvalStatus>,
{
    let [first, second, third] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    callback(*first, *second, *third, values)
}
