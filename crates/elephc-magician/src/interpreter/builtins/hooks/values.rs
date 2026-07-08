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
    eval_abs_result, eval_acos_result, eval_array_aggregate_result, eval_array_flip_result,
    eval_array_keys_result,
    eval_array_mutating_values_result, eval_array_non_mutating_values_result,
    eval_array_pad_result, eval_array_rand_result, eval_array_reverse_result,
    eval_array_search_result, eval_array_slice_result, eval_array_unique_result,
    eval_array_values_result, eval_asin_result, eval_atan2_result, eval_atan_result,
    eval_base64_decode_result, eval_base64_encode_result,
    eval_bin2hex_result, eval_boolval_result, eval_ceil_result, eval_chr_result,
    eval_clamp_result, eval_cos_result, eval_cosh_result, eval_deg2rad_result,
    eval_core_values_result, eval_crc32_result, eval_ctype_result,
    eval_exp_result, eval_fdiv_result, eval_filesystem_values_result, eval_floatval_result,
    eval_floor_result, eval_fmod_result, eval_formatting_values_result,
    eval_gettype_result, eval_hypot_result, eval_intdiv_result, eval_intval_result,
    eval_is_array_result, eval_is_bool_result,
    eval_is_double_result, eval_is_finite_result, eval_is_float_result,
    eval_is_infinite_result, eval_is_int_result, eval_is_integer_result,
    eval_is_iterable_result, eval_is_long_result, eval_is_nan_result,
    eval_is_null_result, eval_is_numeric_result, eval_is_object_result,
    eval_is_real_result, eval_is_resource_result, eval_is_scalar_result,
    eval_is_string_result,
    eval_grapheme_strrev_result, eval_gzip_result, eval_hash_equals_result,
    eval_hash_one_shot_result, eval_hex2bin_result, eval_html_entity_result,
    eval_json_decode_values_result, eval_json_encode_values_result, eval_json_last_error_msg_result,
    eval_json_last_error_result, eval_json_validate_values_result, eval_log2_result,
    eval_log10_result, eval_log_result, eval_max_result, eval_min_result,
    eval_mt_rand_values_result, eval_network_env_values_result, eval_nl2br_result,
    eval_pi_result, eval_pow_result, eval_rad2deg_result, eval_rand_values_result,
    eval_random_int_values_result, eval_range_result, eval_regex_values_result, eval_round_result,
    eval_settype_values_result, eval_sin_result, eval_sinh_result, eval_slashes_result,
    eval_sqrt_result,
    eval_str_pad_result, eval_str_replace_result, eval_str_repeat_result,
    eval_str_split_result, eval_stream_bool_predicate_result, eval_stream_introspection_result,
    eval_string_case_result, eval_string_compare_result, eval_string_position_result,
    eval_string_search_result, eval_strstr_result, eval_strval_result,
    eval_substr_replace_result, eval_substr_result, eval_raw_memory_values_result,
    eval_symbols_values_result, eval_tan_result, eval_tanh_result, eval_time_values_result,
    eval_trim_like_result,
    eval_ucwords_result, eval_url_decode_result, eval_url_encode_result,
    eval_wordwrap_result,
};
use super::arity::{one_arg, three_args, two_args};
use super::hash::{eval_hash_algos_values, eval_hash_context_values};
use super::number_format::eval_number_format_values;
use super::string_split_join::eval_string_split_join_values;

/// Evaluated-argument dispatch hooks for migrated builtins.
#[derive(Clone, Copy)]
pub(in crate::interpreter) enum EvalValuesHook {
    /// Dispatches `abs(...)`.
    Abs,
    /// Dispatches `array_sum(...)` and `array_product(...)`.
    ArrayAggregate,
    /// Dispatches non-mutating array and iterator builtins.
    Array,
    /// Dispatches by-value calls for mutating array builtins.
    ArrayMutating,
    /// Dispatches `array_flip(...)`.
    ArrayFlip,
    /// Dispatches `array_key_exists(...)`.
    ArrayKeyExists,
    /// Dispatches `array_pad(...)`.
    ArrayPad,
    /// Dispatches `array_keys(...)`.
    ArrayKeys,
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
    /// Dispatches `array_values(...)`.
    ArrayValues,
    /// Dispatches `base64_decode(...)`.
    Base64Decode,
    /// Dispatches `base64_encode(...)`.
    Base64Encode,
    /// Dispatches `bin2hex(...)`.
    Bin2Hex,
    /// Dispatches `boolval(...)`.
    Boolval,
    /// Dispatches `ceil(...)`.
    Ceil,
    /// Dispatches `chr(...)`.
    Chr,
    /// Dispatches `clamp(...)`.
    Clamp,
    /// Dispatches `count(...)`.
    Count,
    /// Dispatches core callable, constant, process-control, and debug-output builtins.
    Core,
    /// Dispatches `crc32(...)`.
    Crc32,
    /// Dispatches `ctype_*` predicates.
    Ctype,
    /// Dispatches filesystem and path builtins.
    Filesystem,
    /// Dispatches `acos(...)`.
    Acos,
    /// Dispatches `asin(...)`.
    Asin,
    /// Dispatches `atan(...)`.
    Atan,
    /// Dispatches `atan2(...)`.
    Atan2,
    /// Dispatches `cos(...)`.
    Cos,
    /// Dispatches `cosh(...)`.
    Cosh,
    /// Dispatches `deg2rad(...)`.
    Deg2rad,
    /// Dispatches `exp(...)`.
    Exp,
    /// Dispatches `fdiv(...)`.
    Fdiv,
    /// Dispatches `fmod(...)`.
    Fmod,
    /// Dispatches `hypot(...)`.
    Hypot,
    /// Dispatches printf-family formatting builtins.
    Formatting,
    /// Dispatches `floor(...)`.
    Floor,
    /// Dispatches `gettype(...)`.
    Gettype,
    /// Dispatches `floatval(...)`.
    Floatval,
    /// Dispatches `intval(...)`.
    Intval,
    /// Dispatches `is_array(...)`.
    IsArray,
    /// Dispatches `is_bool(...)`.
    IsBool,
    /// Dispatches `is_double(...)`.
    IsDouble,
    /// Dispatches `is_finite(...)`.
    IsFinite,
    /// Dispatches `is_float(...)`.
    IsFloat,
    /// Dispatches `is_infinite(...)`.
    IsInfinite,
    /// Dispatches `is_int(...)`.
    IsInt,
    /// Dispatches `is_integer(...)`.
    IsInteger,
    /// Dispatches `is_iterable(...)`.
    IsIterable,
    /// Dispatches `is_long(...)`.
    IsLong,
    /// Dispatches `is_nan(...)`.
    IsNan,
    /// Dispatches `is_null(...)`.
    IsNull,
    /// Dispatches `is_numeric(...)`.
    IsNumeric,
    /// Dispatches `is_object(...)`.
    IsObject,
    /// Dispatches `is_real(...)`.
    IsReal,
    /// Dispatches `is_resource(...)`.
    IsResource,
    /// Dispatches `is_scalar(...)`.
    IsScalar,
    /// Dispatches `is_string(...)`.
    IsString,
    /// Dispatches `grapheme_strrev(...)`.
    GraphemeStrrev,
    /// Dispatches gzip/zlib string builtins.
    Gzip,
    /// Dispatches `hash_algos()`.
    HashAlgos,
    /// Dispatches incremental hash-context builtins.
    HashContext,
    /// Dispatches `hash_equals(...)`.
    HashEquals,
    /// Dispatches one-shot hash digest builtins.
    HashOneShot,
    /// Dispatches `hex2bin(...)`.
    Hex2Bin,
    /// Dispatches HTML entity encode/decode builtins.
    HtmlEntity,
    /// Dispatches `intdiv(...)`.
    Intdiv,
    /// Dispatches `json_decode(...)`.
    JsonDecode,
    /// Dispatches `json_encode(...)`.
    JsonEncode,
    /// Dispatches `json_last_error()`.
    JsonLastError,
    /// Dispatches `json_last_error_msg()`.
    JsonLastErrorMsg,
    /// Dispatches `json_validate(...)`.
    JsonValidate,
    /// Dispatches `log(...)`.
    Log,
    /// Dispatches `log2(...)`.
    Log2,
    /// Dispatches `log10(...)`.
    Log10,
    /// Dispatches `max(...)`.
    Max,
    /// Dispatches `min(...)`.
    Min,
    /// Dispatches network, host, environment, and process builtins.
    NetworkEnv,
    /// Dispatches `number_format(...)`.
    NumberFormat,
    /// Dispatches `ord(...)`.
    Ord,
    /// Dispatches `pi()`.
    Pi,
    /// Dispatches `pow(...)`.
    Pow,
    /// Dispatches `mt_rand(...)`.
    MtRand,
    /// Dispatches `rad2deg(...)`.
    Rad2deg,
    /// Dispatches `rand(...)`.
    Rand,
    /// Dispatches `random_int(...)`.
    RandomInt,
    /// Dispatches `round(...)`.
    Round,
    /// Dispatches `range(...)`.
    Range,
    /// Dispatches regex builtins.
    Regex,
    /// Dispatches raw pointer and buffer extension builtins.
    RawMemory,
    /// Dispatches by-value `settype(...)` callable calls.
    Settype,
    /// Dispatches `addslashes(...)` and `stripslashes(...)`.
    Slashes,
    /// Dispatches `sin(...)`.
    Sin,
    /// Dispatches `sinh(...)`.
    Sinh,
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
    /// Dispatches `explode(...)` and `implode(...)`.
    StringSplitJoin,
    /// Dispatches stream boolean predicate builtins.
    StreamBoolPredicate,
    /// Dispatches stream introspection list builtins.
    StreamIntrospection,
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
    /// Dispatches `strval(...)`.
    Strval,
    /// Dispatches `strrev(...)`.
    Strrev,
    /// Dispatches `strstr(...)`.
    Strstr,
    /// Dispatches `substr(...)`.
    Substr,
    /// Dispatches `substr_replace(...)`.
    SubstrReplace,
    /// Dispatches symbol, class metadata, SPL, and language-construct probes.
    Symbols,
    /// Dispatches `tan(...)`.
    Tan,
    /// Dispatches `tanh(...)`.
    Tanh,
    /// Dispatches date, time, and sleep builtins.
    Time,
    /// Dispatches trim-family builtins.
    TrimLike,
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
            Self::Abs => one_arg(evaluated_args, values, eval_abs_result),
            Self::Acos => one_arg(evaluated_args, values, eval_acos_result),
            Self::ArrayAggregate => one_arg(evaluated_args, values, |array, values| {
                eval_array_aggregate_result(name, array, values)
            }),
            Self::Array => eval_array_non_mutating_values_result(name, evaluated_args, context, values),
            Self::ArrayMutating => {
                eval_array_mutating_values_result(name, evaluated_args, context, values)
            }
            Self::ArrayFlip => one_arg(evaluated_args, values, eval_array_flip_result),
            Self::ArrayKeyExists => two_args(evaluated_args, values, |key, array, values| {
                values.array_key_exists(key, array)
            }),
            Self::ArrayPad => three_args(evaluated_args, values, eval_array_pad_result),
            Self::ArrayKeys => one_arg(evaluated_args, values, eval_array_keys_result),
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
            Self::ArrayValues => one_arg(evaluated_args, values, eval_array_values_result),
            Self::Asin => one_arg(evaluated_args, values, eval_asin_result),
            Self::Atan => one_arg(evaluated_args, values, eval_atan_result),
            Self::Atan2 => two_args(evaluated_args, values, eval_atan2_result),
            Self::Base64Decode => one_arg(evaluated_args, values, eval_base64_decode_result),
            Self::Base64Encode => one_arg(evaluated_args, values, eval_base64_encode_result),
            Self::Bin2Hex => one_arg(evaluated_args, values, eval_bin2hex_result),
            Self::Boolval => one_arg(evaluated_args, values, eval_boolval_result),
            Self::Ceil => one_arg(evaluated_args, values, eval_ceil_result),
            Self::Chr => one_arg(evaluated_args, values, eval_chr_result),
            Self::Clamp => three_args(evaluated_args, values, eval_clamp_result),
            Self::Count => match evaluated_args {
                [value] => eval_count_result(*value, None, context, values),
                [value, mode] => eval_count_result(*value, Some(*mode), context, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Core => eval_core_values_result(name, evaluated_args, context, values),
            Self::Cos => one_arg(evaluated_args, values, eval_cos_result),
            Self::Cosh => one_arg(evaluated_args, values, eval_cosh_result),
            Self::Crc32 => one_arg(evaluated_args, values, eval_crc32_result),
            Self::Ctype => one_arg(evaluated_args, values, |value, values| {
                eval_ctype_result(name, value, values)
            }),
            Self::Deg2rad => one_arg(evaluated_args, values, eval_deg2rad_result),
            Self::Exp => one_arg(evaluated_args, values, eval_exp_result),
            Self::Fdiv => two_args(evaluated_args, values, eval_fdiv_result),
            Self::Filesystem => eval_filesystem_values_result(name, evaluated_args, context, values),
            Self::Floor => one_arg(evaluated_args, values, eval_floor_result),
            Self::Fmod => two_args(evaluated_args, values, eval_fmod_result),
            Self::Formatting => eval_formatting_values_result(name, evaluated_args, values),
            Self::Gettype => one_arg(evaluated_args, values, eval_gettype_result),
            Self::Hypot => two_args(evaluated_args, values, eval_hypot_result),
            Self::Floatval => one_arg(evaluated_args, values, eval_floatval_result),
            Self::Intval => one_arg(evaluated_args, values, eval_intval_result),
            Self::IsArray => one_arg(evaluated_args, values, eval_is_array_result),
            Self::IsBool => one_arg(evaluated_args, values, eval_is_bool_result),
            Self::IsDouble => one_arg(evaluated_args, values, eval_is_double_result),
            Self::IsFinite => one_arg(evaluated_args, values, eval_is_finite_result),
            Self::IsFloat => one_arg(evaluated_args, values, eval_is_float_result),
            Self::IsInfinite => one_arg(evaluated_args, values, eval_is_infinite_result),
            Self::IsInt => one_arg(evaluated_args, values, eval_is_int_result),
            Self::IsInteger => one_arg(evaluated_args, values, eval_is_integer_result),
            Self::IsIterable => one_arg(evaluated_args, values, |value, values| {
                eval_is_iterable_result(value, context, values)
            }),
            Self::IsLong => one_arg(evaluated_args, values, eval_is_long_result),
            Self::IsNan => one_arg(evaluated_args, values, eval_is_nan_result),
            Self::IsNull => one_arg(evaluated_args, values, eval_is_null_result),
            Self::IsNumeric => one_arg(evaluated_args, values, eval_is_numeric_result),
            Self::IsObject => one_arg(evaluated_args, values, eval_is_object_result),
            Self::IsReal => one_arg(evaluated_args, values, eval_is_real_result),
            Self::IsResource => one_arg(evaluated_args, values, eval_is_resource_result),
            Self::IsScalar => one_arg(evaluated_args, values, eval_is_scalar_result),
            Self::IsString => one_arg(evaluated_args, values, eval_is_string_result),
            Self::GraphemeStrrev => one_arg(evaluated_args, values, eval_grapheme_strrev_result),
            Self::Gzip => eval_gzip_result(name, evaluated_args, values),
            Self::HashAlgos => eval_hash_algos_values(evaluated_args, values),
            Self::HashContext => eval_hash_context_values(name, evaluated_args, context, values),
            Self::HashEquals => two_args(evaluated_args, values, eval_hash_equals_result),
            Self::HashOneShot => eval_hash_one_shot_result(name, evaluated_args, values),
            Self::Hex2Bin => one_arg(evaluated_args, values, eval_hex2bin_result),
            Self::HtmlEntity => one_arg(evaluated_args, values, |value, values| {
                eval_html_entity_result(name, value, values)
            }),
            Self::Intdiv => two_args(evaluated_args, values, eval_intdiv_result),
            Self::JsonDecode => eval_json_decode_values_result(evaluated_args, context, values),
            Self::JsonEncode => eval_json_encode_values_result(evaluated_args, context, values),
            Self::JsonLastError => {
                if !evaluated_args.is_empty() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                eval_json_last_error_result(context, values)
            }
            Self::JsonLastErrorMsg => {
                if !evaluated_args.is_empty() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                eval_json_last_error_msg_result(context, values)
            }
            Self::JsonValidate => eval_json_validate_values_result(evaluated_args, context, values),
            Self::Log => match evaluated_args {
                [num] => eval_log_result(*num, None, values),
                [num, base] => eval_log_result(*num, Some(*base), values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Log2 => one_arg(evaluated_args, values, eval_log2_result),
            Self::Log10 => one_arg(evaluated_args, values, eval_log10_result),
            Self::Max => eval_max_result(evaluated_args, values),
            Self::Min => eval_min_result(evaluated_args, values),
            Self::MtRand => eval_mt_rand_values_result(evaluated_args, values),
            Self::NetworkEnv => eval_network_env_values_result(name, evaluated_args, values),
            Self::NumberFormat => eval_number_format_values(evaluated_args, values),
            Self::Ord => one_arg(evaluated_args, values, eval_ord_result),
            Self::Pi => {
                if !evaluated_args.is_empty() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                eval_pi_result(values)
            }
            Self::Pow => two_args(evaluated_args, values, eval_pow_result),
            Self::Rad2deg => one_arg(evaluated_args, values, eval_rad2deg_result),
            Self::Rand => eval_rand_values_result(evaluated_args, values),
            Self::RandomInt => eval_random_int_values_result(evaluated_args, values),
            Self::Round => match evaluated_args {
                [value] => eval_round_result(*value, None, values),
                [value, precision] => eval_round_result(*value, Some(*precision), values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Range => two_args(evaluated_args, values, eval_range_result),
            Self::Regex => eval_regex_values_result(name, evaluated_args, context, values),
            Self::RawMemory => eval_raw_memory_values_result(name, evaluated_args, context, values),
            Self::Settype => eval_settype_values_result(evaluated_args, values),
            Self::Sin => one_arg(evaluated_args, values, eval_sin_result),
            Self::Sinh => one_arg(evaluated_args, values, eval_sinh_result),
            Self::Slashes => one_arg(evaluated_args, values, |value, values| {
                eval_slashes_result(name, value, values)
            }),
            Self::Sqrt => one_arg(evaluated_args, values, eval_sqrt_result),
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
            Self::StringSplitJoin => eval_string_split_join_values(name, evaluated_args, values),
            Self::StreamBoolPredicate => one_arg(evaluated_args, values, |stream, values| {
                eval_stream_bool_predicate_result(name, stream, values)
            }),
            Self::StreamIntrospection => {
                if !evaluated_args.is_empty() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                eval_stream_introspection_result(name, context, values)
            }
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
            Self::Strval => one_arg(evaluated_args, values, |value, values| {
                eval_strval_result(value, context, values)
            }),
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
            Self::Symbols => eval_symbols_values_result(name, evaluated_args, context, values),
            Self::Tan => one_arg(evaluated_args, values, eval_tan_result),
            Self::Tanh => one_arg(evaluated_args, values, eval_tanh_result),
            Self::Time => eval_time_values_result(name, evaluated_args, context, values),
            Self::TrimLike => match evaluated_args {
                [value] => eval_trim_like_result(name, *value, None, values),
                [value, mask] => eval_trim_like_result(name, *value, Some(*mask), values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
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
