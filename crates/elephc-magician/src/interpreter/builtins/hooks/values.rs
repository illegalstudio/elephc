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

use super::super::*;
use super::super::super::{
    ElephcEvalContext, EvalStatus, RuntimeCellHandle, RuntimeValueOps,
};
use super::arity::{one_arg, three_args, two_args};

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
    /// Dispatches `printf(...)`.
    Printf,
    /// Dispatches `sprintf(...)`.
    Sprintf,
    /// Dispatches `sscanf(...)`.
    Sscanf,
    /// Dispatches `vprintf(...)`.
    Vprintf,
    /// Dispatches `vsprintf(...)`.
    Vsprintf,
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
    /// Dispatches `random_bytes(...)`.
    RandomBytes,
    /// Dispatches `random_int(...)`.
    RandomInt,
    /// Dispatches `round(...)`.
    Round,
    /// Dispatches `range(...)`.
    Range,
    /// Dispatches `mb_ereg_match(...)`.
    MbEregMatch,
    /// Dispatches `preg_match(...)`.
    PregMatch,
    /// Dispatches `preg_match_all(...)`.
    PregMatchAll,
    /// Dispatches `preg_replace(...)`.
    PregReplace,
    /// Dispatches `preg_replace_callback(...)`.
    PregReplaceCallback,
    /// Dispatches `preg_split(...)`.
    PregSplit,
    /// Dispatches `buffer_free(...)`.
    BufferFree,
    /// Dispatches `buffer_len(...)`.
    BufferLen,
    /// Dispatches `buffer_new(...)`.
    BufferNew,
    /// Dispatches `ptr(...)`.
    Ptr,
    /// Dispatches `ptr_get(...)`.
    PtrGet,
    /// Dispatches `ptr_is_null(...)`.
    PtrIsNull,
    /// Dispatches `ptr_null()`.
    PtrNull,
    /// Dispatches `ptr_offset(...)`.
    PtrOffset,
    /// Dispatches `ptr_read8(...)`.
    PtrRead8,
    /// Dispatches `ptr_read16(...)`.
    PtrRead16,
    /// Dispatches `ptr_read32(...)`.
    PtrRead32,
    /// Dispatches `ptr_read_string(...)`.
    PtrReadString,
    /// Dispatches `ptr_set(...)`.
    PtrSet,
    /// Dispatches `ptr_sizeof(...)`.
    PtrSizeof,
    /// Dispatches `ptr_write8(...)`.
    PtrWrite8,
    /// Dispatches `ptr_write16(...)`.
    PtrWrite16,
    /// Dispatches `ptr_write32(...)`.
    PtrWrite32,
    /// Dispatches `ptr_write_string(...)`.
    PtrWriteString,
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
    /// Dispatches `strlen(...)` and `mb_strlen(...)`.
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
            Self::ArrayAggregate
            | Self::Array
            | Self::ArrayMutating
            | Self::ArrayFlip
            | Self::ArrayKeyExists
            | Self::ArrayPad
            | Self::ArrayKeys
            | Self::ArrayRand
            | Self::ArrayReverse
            | Self::ArraySearch
            | Self::ArraySlice
            | Self::ArrayUnique
            | Self::ArrayValues
            | Self::Count
            | Self::Range => {
                eval_array_declared_values_result(name, evaluated_args, context, values)
            }
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
            Self::Core => eval_core_values_result(name, evaluated_args, context, values),
            Self::Cos => one_arg(evaluated_args, values, eval_cos_result),
            Self::Cosh => one_arg(evaluated_args, values, eval_cosh_result),
            Self::Crc32 => one_arg(evaluated_args, values, eval_crc32_result),
            Self::Ctype => one_arg(evaluated_args, values, |value, values| match name {
                "ctype_alnum" => eval_ctype_alnum_result(value, values),
                "ctype_alpha" => eval_ctype_alpha_result(value, values),
                "ctype_digit" => eval_ctype_digit_result(value, values),
                "ctype_space" => eval_ctype_space_result(value, values),
                _ => Err(EvalStatus::RuntimeFatal),
            }),
            Self::Deg2rad => one_arg(evaluated_args, values, eval_deg2rad_result),
            Self::Exp => one_arg(evaluated_args, values, eval_exp_result),
            Self::Fdiv => two_args(evaluated_args, values, eval_fdiv_result),
            Self::Filesystem => eval_filesystem_values_result(name, evaluated_args, context, values),
            Self::Floor => one_arg(evaluated_args, values, eval_floor_result),
            Self::Fmod => two_args(evaluated_args, values, eval_fmod_result),
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
            Self::Gzip => match name {
                "gzcompress" => eval_gzcompress_result(evaluated_args, values),
                "gzdeflate" => eval_gzdeflate_result(evaluated_args, values),
                "gzinflate" => eval_gzinflate_result(evaluated_args, values),
                "gzuncompress" => eval_gzuncompress_result(evaluated_args, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::HashAlgos => eval_hash_algos_declared_values_result(evaluated_args, values),
            Self::HashContext => match name {
                "hash_copy" => {
                    eval_hash_copy_declared_values_result(evaluated_args, context, values)
                }
                "hash_final" => {
                    eval_hash_final_declared_values_result(evaluated_args, context, values)
                }
                "hash_init" => {
                    eval_hash_init_declared_values_result(evaluated_args, context, values)
                }
                "hash_update" => {
                    eval_hash_update_declared_values_result(evaluated_args, context, values)
                }
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::HashEquals => two_args(evaluated_args, values, eval_hash_equals_result),
            Self::HashOneShot => match name {
                "hash" => eval_hash_result(evaluated_args, values),
                "hash_file" => eval_hash_file_result(evaluated_args, values),
                "hash_hmac" => eval_hash_hmac_result(evaluated_args, values),
                "md5" => eval_md5_result(evaluated_args, values),
                "sha1" => eval_sha1_result(evaluated_args, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Hex2Bin => one_arg(evaluated_args, values, eval_hex2bin_result),
            Self::HtmlEntity => {
                // htmlspecialchars/htmlentities accept optional flags/encoding args;
                // like the static runtime they are accepted without effect (ENT_QUOTES).
                let value = match (name, evaluated_args) {
                    (_, [value]) => *value,
                    ("htmlspecialchars" | "htmlentities", [value, _flags]) => *value,
                    ("htmlspecialchars" | "htmlentities", [value, _flags, _encoding]) => *value,
                    _ => return Err(EvalStatus::RuntimeFatal),
                };
                match name {
                    "html_entity_decode" => eval_html_entity_decode_result(value, values),
                    "htmlentities" => eval_htmlentities_result(value, values),
                    "htmlspecialchars" => eval_htmlspecialchars_result(value, values),
                    _ => Err(EvalStatus::RuntimeFatal),
                }
            }
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
            Self::NumberFormat => {
                eval_number_format_declared_values_result(evaluated_args, values)
            }
            Self::Ord => one_arg(evaluated_args, values, eval_ord_result),
            Self::Pi => {
                if !evaluated_args.is_empty() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                eval_pi_result(values)
            }
            Self::Printf => eval_printf_result(evaluated_args, values),
            Self::Pow => two_args(evaluated_args, values, eval_pow_result),
            Self::Rad2deg => one_arg(evaluated_args, values, eval_rad2deg_result),
            Self::Rand => eval_rand_values_result(evaluated_args, values),
            Self::RandomBytes => eval_random_bytes_values_result(evaluated_args, values),
            Self::RandomInt => eval_random_int_values_result(evaluated_args, values),
            Self::Round => match evaluated_args {
                [value] => eval_round_result(*value, None, values),
                [value, precision] => eval_round_result(*value, Some(*precision), values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::MbEregMatch => eval_mb_ereg_match_values_result(evaluated_args, values),
            Self::PregMatch => eval_preg_match_values_result(evaluated_args, values),
            Self::PregMatchAll => eval_preg_match_all_values_result(evaluated_args, values),
            Self::PregReplace => eval_preg_replace_values_result(evaluated_args, values),
            Self::PregReplaceCallback => {
                eval_preg_replace_callback_values_result(evaluated_args, context, values)
            }
            Self::PregSplit => eval_preg_split_values_result(evaluated_args, values),
            Self::BufferFree => eval_buffer_free_values_result(evaluated_args, values),
            Self::BufferLen => eval_buffer_len_values_result(evaluated_args, values),
            Self::BufferNew => eval_buffer_new_values_result(evaluated_args, values),
            Self::Ptr => eval_ptr_values_result(evaluated_args),
            Self::PtrGet => eval_ptr_get_values_result(evaluated_args, values),
            Self::PtrIsNull => eval_ptr_is_null_values_result(evaluated_args, values),
            Self::PtrNull => eval_ptr_null_values_result(evaluated_args, values),
            Self::PtrOffset => eval_ptr_offset_values_result(evaluated_args, values),
            Self::PtrRead8 => eval_ptr_read8_values_result(evaluated_args, values),
            Self::PtrRead16 => eval_ptr_read16_values_result(evaluated_args, values),
            Self::PtrRead32 => eval_ptr_read32_values_result(evaluated_args, values),
            Self::PtrReadString => eval_ptr_read_string_values_result(evaluated_args, values),
            Self::PtrSet => eval_ptr_set_values_result(evaluated_args, values),
            Self::PtrSizeof => eval_ptr_sizeof_values_result(evaluated_args, context, values),
            Self::PtrWrite8 => eval_ptr_write8_values_result(evaluated_args, values),
            Self::PtrWrite16 => eval_ptr_write16_values_result(evaluated_args, values),
            Self::PtrWrite32 => eval_ptr_write32_values_result(evaluated_args, values),
            Self::PtrWriteString => eval_ptr_write_string_values_result(evaluated_args, values),
            Self::Settype => eval_settype_values_result(evaluated_args, values),
            Self::Sin => one_arg(evaluated_args, values, eval_sin_result),
            Self::Sinh => one_arg(evaluated_args, values, eval_sinh_result),
            Self::Slashes => one_arg(evaluated_args, values, |value, values| match name {
                "addslashes" => eval_addslashes_result(value, values),
                "escapeshellarg" => eval_escapeshellarg_result(value, values),
                "escapeshellcmd" => eval_escapeshellcmd_result(value, values),
                "stripslashes" => eval_stripslashes_result(value, values),
                _ => Err(EvalStatus::RuntimeFatal),
            }),
            Self::Sprintf => eval_sprintf_result(evaluated_args, values),
            Self::Sqrt => one_arg(evaluated_args, values, eval_sqrt_result),
            Self::Sscanf => eval_sscanf_values_result(evaluated_args, values),
            Self::StringCase => one_arg(evaluated_args, values, |value, values| match name {
                "lcfirst" => eval_lcfirst_result(value, values),
                "strtolower" => eval_strtolower_result(value, values),
                "strtoupper" => eval_strtoupper_result(value, values),
                "ucfirst" => eval_ucfirst_result(value, values),
                _ => Err(EvalStatus::RuntimeFatal),
            }),
            Self::StringCompare => two_args(evaluated_args, values, |left, right, values| {
                match name {
                    "strcasecmp" => eval_strcasecmp_result(left, right, values),
                    "strcmp" => eval_strcmp_result(left, right, values),
                    _ => Err(EvalStatus::RuntimeFatal),
                }
            }),
            Self::StringPosition => two_args(evaluated_args, values, |haystack, needle, values| {
                match name {
                    "strpos" => eval_strpos_result(haystack, needle, values),
                    "strrpos" => eval_strrpos_result(haystack, needle, values),
                    _ => Err(EvalStatus::RuntimeFatal),
                }
            }),
            Self::StringSearch => two_args(evaluated_args, values, |haystack, needle, values| {
                match name {
                    "str_contains" => eval_str_contains_result(haystack, needle, values),
                    "str_ends_with" => eval_str_ends_with_result(haystack, needle, values),
                    "str_starts_with" => eval_str_starts_with_result(haystack, needle, values),
                    _ => Err(EvalStatus::RuntimeFatal),
                }
            }),
            Self::StringSplitJoin => match name {
                "explode" => eval_explode_declared_values_result(evaluated_args, values),
                "implode" => eval_implode_declared_values_result(evaluated_args, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StreamBoolPredicate => one_arg(evaluated_args, values, |stream, values| {
                match name {
                    "stream_is_local" => eval_stream_is_local_result(stream, values),
                    "stream_supports_lock" => eval_stream_supports_lock_result(stream, values),
                    _ => Err(EvalStatus::RuntimeFatal),
                }
            }),
            Self::StreamIntrospection => {
                if !evaluated_args.is_empty() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                match name {
                    "stream_get_filters" => eval_stream_get_filters_result(context, values),
                    "stream_get_transports" => eval_stream_get_transports_result(context, values),
                    "stream_get_wrappers" => eval_stream_get_wrappers_result(context, values),
                    _ => Err(EvalStatus::RuntimeFatal),
                }
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
            Self::StrReplace => {
                three_args(evaluated_args, values, |search, replace, subject, values| {
                    match name {
                        "str_ireplace" => {
                            eval_str_ireplace_result(search, replace, subject, values)
                        }
                        "str_replace" => {
                            eval_str_replace_result(name, search, replace, subject, values)
                        }
                        _ => Err(EvalStatus::RuntimeFatal),
                    }
                })
            }
            Self::StrSplit => match evaluated_args {
                [value] => eval_str_split_result(*value, None, values),
                [value, length] => eval_str_split_result(*value, Some(*length), values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Strlen => match name {
                "mb_strlen" => match evaluated_args {
                    [value] => eval_mb_strlen_result(*value, None, context, values),
                    [value, encoding] => {
                        eval_mb_strlen_result(*value, Some(*encoding), context, values)
                    }
                    _ => Err(EvalStatus::RuntimeFatal),
                },
                "strlen" => one_arg(evaluated_args, values, eval_strlen_result),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StrRepeat => two_args(evaluated_args, values, eval_str_repeat_result),
            Self::Strval => one_arg(evaluated_args, values, |value, values| {
                eval_strval_result(value, context, values)
            }),
            Self::Strrev => one_arg(evaluated_args, values, eval_strrev_result),
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
            Self::TrimLike => match (name, evaluated_args) {
                ("chop", [value]) => eval_chop_result(*value, None, values),
                ("chop", [value, mask]) => eval_chop_result(*value, Some(*mask), values),
                ("ltrim", [value]) => eval_ltrim_result(*value, None, values),
                ("ltrim", [value, mask]) => eval_ltrim_result(*value, Some(*mask), values),
                ("rtrim", [value]) => eval_rtrim_result(*value, None, values),
                ("rtrim", [value, mask]) => eval_rtrim_result(*value, Some(*mask), values),
                ("trim", [value]) => eval_trim_result(*value, None, values),
                ("trim", [value, mask]) => eval_trim_result(*value, Some(*mask), values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Ucwords => match evaluated_args {
                [value] => eval_ucwords_result(*value, None, values),
                [value, separators] => eval_ucwords_result(*value, Some(*separators), values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Vprintf => eval_vprintf_result(evaluated_args, values),
            Self::Vsprintf => eval_vsprintf_result(evaluated_args, values),
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
            Self::UrlDecode => one_arg(evaluated_args, values, |value, values| match name {
                "rawurldecode" => eval_rawurldecode_result(value, values),
                "urldecode" => eval_urldecode_result(value, values),
                _ => Err(EvalStatus::RuntimeFatal),
            }),
            Self::UrlEncode => one_arg(evaluated_args, values, |value, values| match name {
                "rawurlencode" => eval_rawurlencode_result(value, values),
                "urlencode" => eval_urlencode_result(value, values),
                _ => Err(EvalStatus::RuntimeFatal),
            }),
        }
    }
}
