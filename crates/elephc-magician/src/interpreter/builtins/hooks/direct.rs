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

use super::super::super::{
    eval_builtin_base64_decode, eval_builtin_base64_encode, eval_builtin_bin2hex,
    eval_builtin_ceil, eval_builtin_chr, eval_builtin_clamp, eval_builtin_count,
    eval_builtin_crc32, eval_builtin_ctype, eval_builtin_explode, eval_builtin_float_binary,
    eval_builtin_float_pair, eval_builtin_float_unary, eval_builtin_floor, eval_builtin_gettype,
    eval_builtin_formatting_call, eval_builtin_gzip, eval_builtin_hash_algos,
    eval_builtin_hash_copy, eval_builtin_hash_final, eval_builtin_hash_init,
    eval_builtin_hash_one_shot, eval_builtin_hash_update, eval_builtin_hex2bin,
    eval_builtin_implode, eval_builtin_intdiv, eval_builtin_log, eval_builtin_min_max,
    eval_builtin_number_format, eval_builtin_ord, eval_builtin_pi, eval_builtin_pow,
    eval_builtin_rand, eval_builtin_random_int, eval_builtin_round, eval_builtin_slashes,
    eval_builtin_sqrt, eval_builtin_str_repeat, eval_builtin_strlen, eval_builtin_type_predicate,
    eval_builtin_url_decode, eval_builtin_url_encode, ElephcEvalContext, ElephcEvalScope,
    EvalExpr, EvalStatus, RuntimeCellHandle, RuntimeValueOps,
};
use super::super::{
    eval_builtin_abs, eval_builtin_array_aggregate, eval_builtin_array_call,
    eval_builtin_array_flip, eval_builtin_array_key_exists, eval_builtin_array_keys,
    eval_builtin_array_pad, eval_builtin_array_rand, eval_builtin_array_reverse,
    eval_builtin_array_search, eval_builtin_array_slice, eval_builtin_array_unique,
    eval_builtin_array_values,
    eval_builtin_cast, eval_builtin_core_call, eval_builtin_filesystem_call,
    eval_builtin_grapheme_strrev, eval_builtin_hash_equals, eval_builtin_html_entity,
    eval_builtin_json_call, eval_builtin_network_env_call, eval_builtin_nl2br, eval_builtin_range,
    eval_builtin_raw_memory_call, eval_builtin_regex_call, eval_builtin_str_pad, eval_builtin_str_replace,
    eval_builtin_str_split, eval_builtin_stream_bool_predicate, eval_builtin_stream_introspection,
    eval_builtin_string_case, eval_builtin_string_compare, eval_builtin_string_position,
    eval_builtin_string_search, eval_builtin_strrev, eval_builtin_strstr, eval_builtin_substr,
    eval_builtin_substr_replace, eval_builtin_symbols_call, eval_builtin_time_call, eval_builtin_trim_like,
    eval_builtin_ucwords, eval_builtin_wordwrap,
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
    /// Dispatches core callable, constant, process-control, and debug-output builtins.
    Core,
    /// Dispatches `crc32(...)`.
    Crc32,
    /// Dispatches `ctype_*` predicates.
    Ctype,
    /// Dispatches filesystem and path builtins.
    Filesystem,
    /// Dispatches binary floating-point builtins.
    FloatBinary,
    /// Dispatches paired floating-point builtins.
    FloatPair,
    /// Dispatches unary floating-point builtins.
    FloatUnary,
    /// Dispatches printf-family formatting builtins.
    Formatting,
    /// Dispatches `floor(...)`.
    Floor,
    /// Dispatches `gettype(...)`.
    Gettype,
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
    /// Dispatches JSON builtins.
    Json,
    /// Dispatches `log(...)`.
    Log,
    /// Dispatches `min(...)` and `max(...)`.
    MinMax,
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
    /// Dispatches random-number builtins.
    Random,
    /// Dispatches `round(...)`.
    Round,
    /// Dispatches `range(...)`.
    Range,
    /// Dispatches regex builtins.
    Regex,
    /// Dispatches raw pointer and buffer extension builtins.
    RawMemory,
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
    /// Dispatches date, time, and sleep builtins.
    Time,
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
            Self::Base64Decode => eval_builtin_base64_decode(args, context, scope, values),
            Self::Base64Encode => eval_builtin_base64_encode(args, context, scope, values),
            Self::Bin2Hex => eval_builtin_bin2hex(args, context, scope, values),
            Self::Cast => eval_builtin_cast(name, args, context, scope, values),
            Self::Ceil => eval_builtin_ceil(args, context, scope, values),
            Self::Chr => eval_builtin_chr(args, context, scope, values),
            Self::Clamp => eval_builtin_clamp(args, context, scope, values),
            Self::Count => eval_builtin_count(args, context, scope, values),
            Self::Core => eval_builtin_core_call(name, args, context, scope, values),
            Self::Crc32 => eval_builtin_crc32(args, context, scope, values),
            Self::Ctype => eval_builtin_ctype(name, args, context, scope, values),
            Self::Filesystem => eval_builtin_filesystem_call(name, args, context, scope, values),
            Self::FloatBinary => eval_builtin_float_binary(name, args, context, scope, values),
            Self::FloatPair => eval_builtin_float_pair(name, args, context, scope, values),
            Self::FloatUnary => eval_builtin_float_unary(name, args, context, scope, values),
            Self::Formatting => eval_builtin_formatting_call(name, args, context, scope, values),
            Self::Floor => eval_builtin_floor(args, context, scope, values),
            Self::Gettype => eval_builtin_gettype(args, context, scope, values),
            Self::GraphemeStrrev => eval_builtin_grapheme_strrev(args, context, scope, values),
            Self::Gzip => eval_builtin_gzip(name, args, context, scope, values),
            Self::HashAlgos => eval_builtin_hash_algos(args, values),
            Self::HashContext => match name {
                "hash_copy" => eval_builtin_hash_copy(args, context, scope, values),
                "hash_final" => eval_builtin_hash_final(args, context, scope, values),
                "hash_init" => eval_builtin_hash_init(args, context, scope, values),
                "hash_update" => eval_builtin_hash_update(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::HashEquals => eval_builtin_hash_equals(args, context, scope, values),
            Self::HashOneShot => eval_builtin_hash_one_shot(name, args, context, scope, values),
            Self::Hex2Bin => eval_builtin_hex2bin(args, context, scope, values),
            Self::HtmlEntity => eval_builtin_html_entity(name, args, context, scope, values),
            Self::Intdiv => eval_builtin_intdiv(args, context, scope, values),
            Self::Json => eval_builtin_json_call(name, args, context, scope, values),
            Self::Log => eval_builtin_log(args, context, scope, values),
            Self::MinMax => eval_builtin_min_max(name, args, context, scope, values),
            Self::NetworkEnv => eval_builtin_network_env_call(name, args, context, scope, values),
            Self::NumberFormat => eval_builtin_number_format(args, context, scope, values),
            Self::Ord => eval_builtin_ord(args, context, scope, values),
            Self::Pi => eval_builtin_pi(args, values),
            Self::Pow => eval_builtin_pow(args, context, scope, values),
            Self::Random => match name {
                "rand" | "mt_rand" => eval_builtin_rand(args, context, scope, values),
                "random_int" => eval_builtin_random_int(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::Round => eval_builtin_round(args, context, scope, values),
            Self::Range => eval_builtin_range(args, context, scope, values),
            Self::Regex => eval_builtin_regex_call(name, args, context, scope, values),
            Self::RawMemory => eval_builtin_raw_memory_call(name, args, context, scope, values),
            Self::Slashes => eval_builtin_slashes(name, args, context, scope, values),
            Self::Sqrt => eval_builtin_sqrt(args, context, scope, values),
            Self::StringCase => eval_builtin_string_case(name, args, context, scope, values),
            Self::StringCompare => eval_builtin_string_compare(name, args, context, scope, values),
            Self::StringPosition => {
                eval_builtin_string_position(name, args, context, scope, values)
            }
            Self::StringSearch => eval_builtin_string_search(name, args, context, scope, values),
            Self::StringSplitJoin => match name {
                "explode" => eval_builtin_explode(args, context, scope, values),
                "implode" => eval_builtin_implode(args, context, scope, values),
                _ => Err(EvalStatus::RuntimeFatal),
            },
            Self::StreamBoolPredicate => {
                eval_builtin_stream_bool_predicate(name, args, context, scope, values)
            }
            Self::StreamIntrospection => {
                eval_builtin_stream_introspection(name, args, context, values)
            }
            Self::StrPad => eval_builtin_str_pad(args, context, scope, values),
            Self::StrReplace => eval_builtin_str_replace(name, args, context, scope, values),
            Self::StrSplit => eval_builtin_str_split(args, context, scope, values),
            Self::Strlen => eval_builtin_strlen(args, context, scope, values),
            Self::StrRepeat => eval_builtin_str_repeat(args, context, scope, values),
            Self::Strrev => eval_builtin_strrev(args, context, scope, values),
            Self::Strstr => eval_builtin_strstr(args, context, scope, values),
            Self::Substr => eval_builtin_substr(args, context, scope, values),
            Self::SubstrReplace => eval_builtin_substr_replace(args, context, scope, values),
            Self::Symbols => eval_builtin_symbols_call(name, args, context, scope, values),
            Self::Time => eval_builtin_time_call(name, args, context, scope, values),
            Self::TrimLike => eval_builtin_trim_like(name, args, context, scope, values),
            Self::TypePredicate => eval_builtin_type_predicate(name, args, context, scope, values),
            Self::Ucwords => eval_builtin_ucwords(args, context, scope, values),
            Self::Nl2br => eval_builtin_nl2br(args, context, scope, values),
            Self::Wordwrap => eval_builtin_wordwrap(args, context, scope, values),
            Self::UrlDecode => eval_builtin_url_decode(name, args, context, scope, values),
            Self::UrlEncode => eval_builtin_url_encode(name, args, context, scope, values),
        }
    }
}
