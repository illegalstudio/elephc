//! Purpose:
//! Direct expression-level dispatch hooks for eval builtins migrated into the
//! declarative registry.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::eval_declared_builtin_direct_call`.
//!
//! Key details:
//! - Direct hooks preserve source-order evaluation in existing builtin helpers.
//! - Hook variants remain static metadata referenced from per-builtin files.

use super::super::*;
use super::super::super::{
    eval_builtin_count, ElephcEvalContext, ElephcEvalScope, EvalExpr, EvalStatus,
    RuntimeCellHandle, RuntimeValueOps,
};

/// Direct expression-level dispatch hooks for migrated builtins.
#[derive(Clone, Copy)]
pub(in crate::interpreter) enum EvalDirectHook {
    /// Dispatches `abs(...)`.
    Abs,
    /// Dispatches `array_sum(...)` and `array_product(...)`.
    ArrayAggregate,
    /// Dispatches non-mutating array and iterator builtins.
    Array,
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
    /// Dispatches `random_int(...)`.
    RandomInt,
    /// Dispatches `round(...)`.
    Round,
    /// Dispatches `range(...)`.
    Range,
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
            Self::Acos => eval_builtin_acos(args, context, scope, values),
            Self::ArrayAggregate => eval_builtin_array_aggregate(name, args, context, scope, values),
            Self::Array => eval_builtin_array_call(name, args, context, scope, values),
            Self::ArrayFlip => eval_builtin_array_flip(args, context, scope, values),
            Self::ArrayKeyExists => eval_builtin_array_key_exists(args, context, scope, values),
            Self::ArrayPad => eval_builtin_array_pad(args, context, scope, values),
            Self::ArrayKeys => eval_builtin_array_keys(args, context, scope, values),
            Self::ArrayRand => eval_builtin_array_rand(args, context, scope, values),
            Self::ArrayReverse => eval_builtin_array_reverse(args, context, scope, values),
            Self::ArraySearch => eval_builtin_array_search(name, args, context, scope, values),
            Self::ArraySlice => eval_builtin_array_slice(args, context, scope, values),
            Self::ArrayUnique => eval_builtin_array_unique(args, context, scope, values),
            Self::ArrayValues => eval_builtin_array_values(args, context, scope, values),
            Self::Asin => eval_builtin_asin(args, context, scope, values),
            Self::Atan => eval_builtin_atan(args, context, scope, values),
            Self::Atan2 => eval_builtin_atan2(args, context, scope, values),
            Self::Base64Decode => eval_builtin_base64_decode(args, context, scope, values),
            Self::Base64Encode => eval_builtin_base64_encode(args, context, scope, values),
            Self::Bin2Hex => eval_builtin_bin2hex(args, context, scope, values),
            Self::Boolval => eval_builtin_boolval(args, context, scope, values),
            Self::Ceil => eval_builtin_ceil(args, context, scope, values),
            Self::Chr => eval_builtin_chr(args, context, scope, values),
            Self::Clamp => eval_builtin_clamp(args, context, scope, values),
            Self::Count => eval_builtin_count(args, context, scope, values),
            Self::Core => eval_builtin_core_call(name, args, context, scope, values),
            Self::Cos => eval_builtin_cos(args, context, scope, values),
            Self::Cosh => eval_builtin_cosh(args, context, scope, values),
            Self::Crc32 => eval_builtin_crc32(args, context, scope, values),
            Self::Ctype => match name {
                "ctype_alnum" => eval_builtin_ctype_alnum(args, context, scope, values),
                "ctype_alpha" => eval_builtin_ctype_alpha(args, context, scope, values),
                "ctype_digit" => eval_builtin_ctype_digit(args, context, scope, values),
                "ctype_space" => eval_builtin_ctype_space(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Deg2rad => eval_builtin_deg2rad(args, context, scope, values),
            Self::Exp => eval_builtin_exp(args, context, scope, values),
            Self::Fdiv => eval_builtin_fdiv(args, context, scope, values),
            Self::Filesystem => eval_builtin_filesystem_call(name, args, context, scope, values),
            Self::Fmod => eval_builtin_fmod(args, context, scope, values),
            Self::Floor => eval_builtin_floor(args, context, scope, values),
            Self::Gettype => eval_builtin_gettype(args, context, scope, values),
            Self::Hypot => eval_builtin_hypot(args, context, scope, values),
            Self::Floatval => eval_builtin_floatval(args, context, scope, values),
            Self::Intval => eval_builtin_intval(args, context, scope, values),
            Self::IsArray => eval_builtin_is_array(args, context, scope, values),
            Self::IsBool => eval_builtin_is_bool(args, context, scope, values),
            Self::IsDouble => eval_builtin_is_double(args, context, scope, values),
            Self::IsFinite => eval_builtin_is_finite(args, context, scope, values),
            Self::IsFloat => eval_builtin_is_float(args, context, scope, values),
            Self::IsInfinite => eval_builtin_is_infinite(args, context, scope, values),
            Self::IsInt => eval_builtin_is_int(args, context, scope, values),
            Self::IsInteger => eval_builtin_is_integer(args, context, scope, values),
            Self::IsIterable => eval_builtin_is_iterable(args, context, scope, values),
            Self::IsLong => eval_builtin_is_long(args, context, scope, values),
            Self::IsNan => eval_builtin_is_nan(args, context, scope, values),
            Self::IsNull => eval_builtin_is_null(args, context, scope, values),
            Self::IsNumeric => eval_builtin_is_numeric(args, context, scope, values),
            Self::IsObject => eval_builtin_is_object(args, context, scope, values),
            Self::IsReal => eval_builtin_is_real(args, context, scope, values),
            Self::IsResource => eval_builtin_is_resource(args, context, scope, values),
            Self::IsScalar => eval_builtin_is_scalar(args, context, scope, values),
            Self::IsString => eval_builtin_is_string(args, context, scope, values),
            Self::GraphemeStrrev => eval_builtin_grapheme_strrev(args, context, scope, values),
            Self::Gzip => match name {
                "gzcompress" => eval_builtin_gzcompress(args, context, scope, values),
                "gzdeflate" => eval_builtin_gzdeflate(args, context, scope, values),
                "gzinflate" => eval_builtin_gzinflate(args, context, scope, values),
                "gzuncompress" => eval_builtin_gzuncompress(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::HashAlgos => eval_builtin_hash_algos(args, values),
            Self::HashContext => match name {
                "hash_copy" => eval_builtin_hash_copy(args, context, scope, values),
                "hash_final" => eval_builtin_hash_final(args, context, scope, values),
                "hash_init" => eval_builtin_hash_init(args, context, scope, values),
                "hash_update" => eval_builtin_hash_update(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::HashEquals => eval_builtin_hash_equals(args, context, scope, values),
            Self::HashOneShot => match name {
                "hash" => eval_builtin_hash(args, context, scope, values),
                "hash_file" => eval_builtin_hash_file(args, context, scope, values),
                "hash_hmac" => eval_builtin_hash_hmac(args, context, scope, values),
                "md5" => eval_builtin_md5(args, context, scope, values),
                "sha1" => eval_builtin_sha1(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Hex2Bin => eval_builtin_hex2bin(args, context, scope, values),
            Self::HtmlEntity => match name {
                "html_entity_decode" => {
                    eval_builtin_html_entity_decode(args, context, scope, values)
                }
                "htmlentities" => eval_builtin_htmlentities(args, context, scope, values),
                "htmlspecialchars" => eval_builtin_htmlspecialchars(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Intdiv => eval_builtin_intdiv(args, context, scope, values),
            Self::JsonDecode => eval_builtin_json_decode(args, context, scope, values),
            Self::JsonEncode => eval_builtin_json_encode(args, context, scope, values),
            Self::JsonLastError => eval_builtin_json_last_error(args, context, values),
            Self::JsonLastErrorMsg => eval_builtin_json_last_error_msg(args, context, values),
            Self::JsonValidate => eval_builtin_json_validate(args, context, scope, values),
            Self::Log => eval_builtin_log(args, context, scope, values),
            Self::Log2 => eval_builtin_log2(args, context, scope, values),
            Self::Log10 => eval_builtin_log10(args, context, scope, values),
            Self::Max => eval_builtin_max(args, context, scope, values),
            Self::Min => eval_builtin_min(args, context, scope, values),
            Self::MtRand => eval_builtin_mt_rand(args, context, scope, values),
            Self::NetworkEnv => eval_builtin_network_env_call(name, args, context, scope, values),
            Self::NumberFormat => eval_builtin_number_format(args, context, scope, values),
            Self::Ord => eval_builtin_ord(args, context, scope, values),
            Self::Pi => eval_builtin_pi(args, values),
            Self::Printf => eval_builtin_printf(args, context, scope, values),
            Self::Pow => eval_builtin_pow(args, context, scope, values),
            Self::Rad2deg => eval_builtin_rad2deg(args, context, scope, values),
            Self::Rand => eval_builtin_rand(args, context, scope, values),
            Self::RandomInt => eval_builtin_random_int(args, context, scope, values),
            Self::Round => eval_builtin_round(args, context, scope, values),
            Self::Range => eval_builtin_range(args, context, scope, values),
            Self::PregMatch => eval_builtin_preg_match(args, context, scope, values),
            Self::PregMatchAll => eval_builtin_preg_match_all(args, context, scope, values),
            Self::PregReplace => eval_builtin_preg_replace(args, context, scope, values),
            Self::PregReplaceCallback => {
                eval_builtin_preg_replace_callback(args, context, scope, values)
            }
            Self::PregSplit => eval_builtin_preg_split(args, context, scope, values),
            Self::BufferFree => eval_builtin_buffer_free(args, context, scope, values),
            Self::BufferLen => eval_builtin_buffer_len(args, context, scope, values),
            Self::BufferNew => eval_builtin_buffer_new(args, context, scope, values),
            Self::Ptr => eval_builtin_ptr(args, context, scope, values),
            Self::PtrGet => eval_builtin_ptr_get(args, context, scope, values),
            Self::PtrIsNull => eval_builtin_ptr_is_null(args, context, scope, values),
            Self::PtrNull => eval_builtin_ptr_null(args, context, scope, values),
            Self::PtrOffset => eval_builtin_ptr_offset(args, context, scope, values),
            Self::PtrRead8 => eval_builtin_ptr_read8(args, context, scope, values),
            Self::PtrRead16 => eval_builtin_ptr_read16(args, context, scope, values),
            Self::PtrRead32 => eval_builtin_ptr_read32(args, context, scope, values),
            Self::PtrReadString => eval_builtin_ptr_read_string(args, context, scope, values),
            Self::PtrSet => eval_builtin_ptr_set(args, context, scope, values),
            Self::PtrSizeof => eval_builtin_ptr_sizeof(args, context, scope, values),
            Self::PtrWrite8 => eval_builtin_ptr_write8(args, context, scope, values),
            Self::PtrWrite16 => eval_builtin_ptr_write16(args, context, scope, values),
            Self::PtrWrite32 => eval_builtin_ptr_write32(args, context, scope, values),
            Self::PtrWriteString => eval_builtin_ptr_write_string(args, context, scope, values),
            Self::Sin => eval_builtin_sin(args, context, scope, values),
            Self::Sinh => eval_builtin_sinh(args, context, scope, values),
            Self::Slashes => match name {
                "addslashes" => eval_builtin_addslashes(args, context, scope, values),
                "stripslashes" => eval_builtin_stripslashes(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Sqrt => super::super::math::eval_builtin_sqrt(args, context, scope, values),
            Self::Sprintf => eval_builtin_sprintf(args, context, scope, values),
            Self::Sscanf => eval_builtin_sscanf(args, context, scope, values),
            Self::StringCase => match name {
                "lcfirst" => eval_builtin_lcfirst(args, context, scope, values),
                "strtolower" => eval_builtin_strtolower(args, context, scope, values),
                "strtoupper" => eval_builtin_strtoupper(args, context, scope, values),
                "ucfirst" => eval_builtin_ucfirst(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StringCompare => match name {
                "strcasecmp" => eval_builtin_strcasecmp(args, context, scope, values),
                "strcmp" => eval_builtin_strcmp(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StringPosition => match name {
                "strpos" => eval_builtin_strpos(args, context, scope, values),
                "strrpos" => eval_builtin_strrpos(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StringSearch => match name {
                "str_contains" => eval_builtin_str_contains(args, context, scope, values),
                "str_ends_with" => eval_builtin_str_ends_with(args, context, scope, values),
                "str_starts_with" => eval_builtin_str_starts_with(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StringSplitJoin => match name {
                "explode" => eval_builtin_explode(args, context, scope, values),
                "implode" => eval_builtin_implode(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StreamBoolPredicate => match name {
                "stream_is_local" => eval_builtin_stream_is_local(args, context, scope, values),
                "stream_supports_lock" => {
                    eval_builtin_stream_supports_lock(args, context, scope, values)
                }
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StreamIntrospection => match name {
                "stream_get_filters" => eval_builtin_stream_get_filters(args, context, values),
                "stream_get_transports" => {
                    eval_builtin_stream_get_transports(args, context, values)
                }
                "stream_get_wrappers" => eval_builtin_stream_get_wrappers(args, context, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StrPad => eval_builtin_str_pad(args, context, scope, values),
            Self::StrReplace => match name {
                "str_ireplace" => eval_builtin_str_ireplace(args, context, scope, values),
                "str_replace" => eval_builtin_str_replace(name, args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StrSplit => eval_builtin_str_split(args, context, scope, values),
            Self::Strlen => eval_builtin_strlen(args, context, scope, values),
            Self::StrRepeat => eval_builtin_str_repeat(args, context, scope, values),
            Self::Strval => eval_builtin_strval(args, context, scope, values),
            Self::Strrev => eval_builtin_strrev(args, context, scope, values),
            Self::Strstr => eval_builtin_strstr(args, context, scope, values),
            Self::Substr => eval_builtin_substr(args, context, scope, values),
            Self::SubstrReplace => eval_builtin_substr_replace(args, context, scope, values),
            Self::Symbols => eval_builtin_symbols_call(name, args, context, scope, values),
            Self::Tan => eval_builtin_tan(args, context, scope, values),
            Self::Tanh => eval_builtin_tanh(args, context, scope, values),
            Self::Time => eval_builtin_time_call(name, args, context, scope, values),
            Self::TrimLike => match name {
                "chop" => eval_builtin_chop(args, context, scope, values),
                "ltrim" => eval_builtin_ltrim(args, context, scope, values),
                "rtrim" => eval_builtin_rtrim(args, context, scope, values),
                "trim" => eval_builtin_trim(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Ucwords => eval_builtin_ucwords(args, context, scope, values),
            Self::Vprintf => eval_builtin_vprintf(args, context, scope, values),
            Self::Vsprintf => eval_builtin_vsprintf(args, context, scope, values),
            Self::Nl2br => eval_builtin_nl2br(args, context, scope, values),
            Self::Wordwrap => eval_builtin_wordwrap(args, context, scope, values),
            Self::UrlDecode => match name {
                "rawurldecode" => eval_builtin_rawurldecode(args, context, scope, values),
                "urldecode" => eval_builtin_urldecode(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::UrlEncode => match name {
                "rawurlencode" => eval_builtin_rawurlencode(args, context, scope, values),
                "urlencode" => eval_builtin_urlencode(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
        }
    }
}
