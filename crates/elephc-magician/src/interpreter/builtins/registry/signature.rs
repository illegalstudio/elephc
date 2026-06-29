//! Purpose:
//! Signature-shape metadata for PHP-visible eval builtin calls.
//! Keeps named/default/variadic/by-reference shape visible to parity tests
//! without duplicating runtime dispatch behavior.
//!
//! Called from:
//! - `crate::interpreter::builtin_metadata`
//! - builtin registry tests and argument binding audits.
//!
//! Key details:
//! - Parameter names come from `eval_builtin_param_names()`.
//! - Default values mirror the dispatcher defaults so named calls can skip
//!   optional parameters without changing positional semantics.

use super::eval_builtin_param_names;

/// Comparison-friendly shape for one eval builtin signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::interpreter) struct EvalBuiltinSignatureShape {
    /// Number of leading parameters that must be supplied.
    pub(in crate::interpreter) required_param_count: usize,
    /// Number of parameters that have defaults.
    pub(in crate::interpreter) default_param_count: usize,
    /// Variadic parameter name when this builtin accepts extra arguments.
    pub(in crate::interpreter) variadic: Option<&'static str>,
    /// Parameter names that are passed by reference.
    pub(in crate::interpreter) by_ref_params: &'static [&'static str],
}

/// Runtime-materializable default value for one eval builtin parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::interpreter) enum EvalBuiltinDefaultValue {
    /// PHP null default.
    Null,
    /// PHP boolean default.
    Bool(bool),
    /// PHP integer default.
    Int(i64),
    /// PHP float default.
    Float(f64),
    /// PHP string default represented as UTF-8 text.
    String(&'static str),
    /// PHP string default represented as raw bytes.
    Bytes(&'static [u8]),
    /// PHP empty indexed array default.
    EmptyArray,
}

/// Returns signature-shape metadata for one PHP-visible eval builtin.
pub(in crate::interpreter) fn eval_builtin_signature_shape(
    name: &str,
) -> Option<EvalBuiltinSignatureShape> {
    let params = eval_builtin_param_names(name)?;
    Some(match name {
        "gzcompress" | "gzdeflate" | "gzinflate" | "gzuncompress" => optional(params, 1),

        "isset" | "unset" => variadic(params, &[]),
        "settype" => fixed_by_ref(params, &["var"]),

        "class_alias" => optional(params, 2),
        "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
        | "class_implements" | "class_parents" | "class_uses" => optional(params, 1),
        "iterator_to_array" => optional(params, 1),
        "iterator_apply" => optional(params, 2),
        "get_class" | "get_parent_class" => optional(params, 0),
        "is_a" | "is_subclass_of" => optional(params, 2),

        "count" => optional(params, 1),
        "getdate" | "hrtime" => optional(params, 0),
        "header" => optional(params, 1),
        "http_response_code" => optional(params, 0),
        "localtime" => optional(params, 0),
        "microtime" | "php_uname" | "readline" | "umask" | "exit" | "die" => {
            optional(params, 0)
        }

        "trim" | "ltrim" | "rtrim" | "chop" | "ucwords" | "str_split" | "wordwrap" => {
            optional(params, 1)
        }
        "substr" | "strpos" | "strrpos" | "strstr" | "explode" | "str_pad" => {
            optional(params, 2)
        }
        "str_replace" | "str_ireplace" => optional(params, 3),
        "implode" => optional(params, 1),
        "substr_replace" => optional(params, 3),
        "sprintf" | "printf" | "sscanf" => variadic(params, &[]),
        "fprintf" | "fscanf" => variadic(params, &[]),

        "hash" | "hash_file" => optional(params, 2),
        "hash_hmac" => optional(params, 3),
        "hash_init" => optional(params, 1),
        "hash_final" | "md5" | "sha1" => optional(params, 1),
        "number_format" => optional(params, 1),

        "array_pop" | "array_shift" => fixed_by_ref(params, &["array"]),
        "array_reverse" => optional(params, 1),
        "sort" | "rsort" | "shuffle" | "natsort" | "natcasesort" | "asort" | "arsort"
        | "ksort" | "krsort" => fixed_by_ref(params, &["array"]),
        "in_array" | "array_search" => optional(params, 2),
        "array_push" | "array_unshift" => variadic(params, &["array"]),
        "array_merge" => variadic(params, &[]),
        "array_diff" | "array_intersect" | "array_diff_key" | "array_intersect_key" => {
            variadic(params, &[])
        }
        "array_slice" => optional(params, 2),
        "array_splice" => optional_by_ref(params, 2, &["array"]),
        "array_map" => variadic(params, &[]),
        "array_filter" => optional(params, 1),
        "array_reduce" => optional(params, 2),
        "array_walk" | "usort" | "uksort" | "uasort" => fixed_by_ref(params, &["array"]),
        "call_user_func" => variadic(params, &[]),

        "log" | "round" | "date" | "gmdate" | "nl2br" => optional(params, 1),
        "min" | "max" => variadic(params, &[]),
        "json_encode" | "json_decode" | "json_validate" => optional(params, 1),

        "preg_match" => optional_by_ref(params, 2, &["matches"]),
        "preg_split" => optional(params, 2),
        "print_r" => optional(params, 1),
        "var_dump" => variadic(params, &[]),

        "touch" | "basename" | "dirname" | "pathinfo" => optional(params, 1),
        "fnmatch" | "fopen" | "fseek" | "fputcsv" => optional(params, 2),
        "flock" => optional_by_ref(params, 2, &["would_block"]),
        "fgetcsv" => optional(params, 1),
        "clearstatcache" => optional(params, 0),
        "stream_get_contents" => optional(params, 1),
        "stream_copy_to_stream" => optional(params, 2),
        "stream_socket_accept" => optional_by_ref(params, 1, &["peer_name"]),
        "fsockopen" | "pfsockopen" => {
            optional_by_ref(params, 2, &["error_code", "error_message"])
        }
        "stream_wrapper_register" | "stream_socket_enable_crypto" => optional(params, 2),
        "stream_context_create" | "stream_context_get_default" => optional(params, 0),
        "stream_context_set_option" => optional(params, 2),
        "stream_get_line" | "stream_set_timeout" | "stream_socket_sendto"
        | "stream_filter_append" | "stream_filter_prepend" => optional(params, 2),
        "stream_select" => optional_by_ref(params, 4, &["read", "write", "except"]),
        "stream_socket_recvfrom" => optional_by_ref(params, 2, &["address"]),

        "spl_autoload_register" | "spl_autoload_extensions" => optional(params, 0),
        "spl_autoload" => optional(params, 1),

        _ => fixed(params),
    })
}

/// Returns the concrete default value for one optional builtin parameter.
pub(in crate::interpreter) fn eval_builtin_default_value(
    name: &str,
    param_index: usize,
) -> Option<EvalBuiltinDefaultValue> {
    use EvalBuiltinDefaultValue::*;

    Some(match (name, param_index) {
        ("gzcompress" | "gzdeflate", 1) => Int(-1),
        ("gzinflate" | "gzuncompress", 1) => Int(0),

        ("class_alias", 2) => Bool(true),
        (
            "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
            | "class_implements" | "class_parents" | "class_uses",
            1,
        ) => Bool(true),
        ("iterator_to_array", 1) => Bool(true),
        ("iterator_apply", 2) => Null,
        ("get_class" | "get_parent_class", 0) => Null,
        ("is_a", 2) => Bool(false),
        ("is_subclass_of", 2) => Bool(true),

        ("count", 1) => Int(0),
        ("getdate", 0) => Null,
        ("header", 1) => Bool(true),
        ("header", 2) => Int(0),
        ("hrtime", 0) => Bool(false),
        ("http_response_code", 0) => Int(0),
        ("localtime", 0) => Null,
        ("localtime", 1) => Bool(false),
        ("microtime", 0) => Bool(false),
        ("php_uname", 0) => String("a"),
        ("readline" | "umask", 0) => Null,
        ("exit" | "die", 0) => Int(0),

        ("trim" | "ltrim" | "rtrim" | "chop", 1) => Bytes(b" \n\r\t\x0b\x0c\0"),
        ("ucwords", 1) => Bytes(b" \t\r\n\x0c\x0b"),
        ("substr", 2) => Null,
        ("strpos" | "strrpos", 2) => Int(0),
        ("strstr", 2) => Bool(false),
        ("str_replace" | "str_ireplace", 3) => Null,
        ("explode", 2) => Int(i64::MAX),
        ("implode", 0) => Null,
        ("substr_replace", 3) => Null,
        ("str_pad", 2) => String(" "),
        ("str_pad", 3) => Int(1),
        ("str_split", 1) => Int(1),
        ("wordwrap", 1) => Int(75),
        ("wordwrap", 2) => String("\n"),
        ("wordwrap", 3) => Bool(false),

        ("hash" | "hash_file", 2) => Bool(false),
        ("hash_hmac", 3) => Bool(false),
        ("hash_init", 1) => Int(0),
        ("hash_init", 2) => String(""),
        ("hash_final" | "md5" | "sha1", 1) => Bool(false),
        ("number_format", 1) => Int(0),
        ("number_format", 2) => String("."),
        ("number_format", 3) => String(","),

        ("array_reverse", 1) => Bool(false),
        ("in_array" | "array_search", 2) => Bool(false),
        ("array_slice" | "array_splice", 2) => Null,
        ("array_splice", 3) => EmptyArray,
        ("array_filter", 1) => Null,
        ("array_filter", 2) => Int(0),
        ("array_reduce", 2) => Null,

        ("log", 1) => Float(std::f64::consts::E),
        ("round", 1) => Int(0),
        ("date" | "gmdate", 1) => Null,
        ("nl2br", 1) => Bool(true),
        ("json_encode", 1) => Int(0),
        ("json_encode", 2) => Int(512),
        ("json_decode", 1) => Null,
        ("json_decode", 2) => Int(512),
        ("json_decode", 3) => Int(0),
        ("json_validate", 1) => Int(512),
        ("json_validate", 2) => Int(0),

        ("preg_match", 2) => EmptyArray,
        ("preg_split", 2) => Int(-1),
        ("preg_split", 3) => Int(0),
        ("print_r", 1) => Bool(false),

        ("touch", 1 | 2) => Null,
        ("basename", 1) => String(""),
        ("dirname", 1) => Int(1),
        ("fnmatch", 2) => Int(0),
        ("pathinfo", 1) => Int(15),
        ("fopen", 2) => Bool(false),
        ("fopen", 3) => Null,
        ("flock", 2) => Null,
        ("fseek", 2) => Int(0),
        ("fgetcsv", 1) => Null,
        ("fgetcsv", 2) => String(","),
        ("fputcsv", 2) => String(","),
        ("fputcsv", 3) => String("\""),
        ("clearstatcache", 0) => Bool(false),
        ("clearstatcache", 1) => String(""),
        ("stream_get_contents", 1) => Null,
        ("stream_get_contents", 2) => Int(-1),
        ("stream_copy_to_stream", 2) => Null,
        ("stream_copy_to_stream", 3) => Int(-1),
        ("stream_socket_accept", 1 | 2) => Null,
        ("fsockopen" | "pfsockopen", 2 | 3 | 4) => Null,
        ("stream_wrapper_register", 2) => Int(0),
        ("stream_socket_enable_crypto", 2 | 3) => Null,
        ("stream_context_create", 0 | 1) => Null,
        ("stream_context_get_default", 0) => Null,
        ("stream_context_set_option", 2 | 3) => Null,
        ("stream_get_line", 2) => String(""),
        ("stream_select", 4) => Int(0),
        ("stream_set_timeout", 2) => Int(0),
        ("stream_socket_sendto", 2) => Int(0),
        ("stream_socket_sendto", 3) => String(""),
        ("stream_socket_recvfrom", 2) => Int(0),
        ("stream_socket_recvfrom", 3) => String(""),
        ("stream_filter_append" | "stream_filter_prepend", 2) => Int(3),
        ("stream_filter_append" | "stream_filter_prepend", 3) => Null,

        ("spl_autoload_register", 0) => Null,
        ("spl_autoload_register", 1) => Bool(true),
        ("spl_autoload_register", 2) => Bool(false),
        ("spl_autoload_extensions", 0) => Null,
        ("spl_autoload", 1) => Null,

        _ => return None,
    })
}

/// Builds fixed-arity signature shape.
fn fixed(params: &[&'static str]) -> EvalBuiltinSignatureShape {
    shape(params.len(), 0, None, &[])
}

/// Builds fixed-arity signature shape with by-reference parameters.
fn fixed_by_ref(
    params: &[&'static str],
    by_ref_params: &'static [&'static str],
) -> EvalBuiltinSignatureShape {
    shape(params.len(), 0, None, by_ref_params)
}

/// Builds trailing-default signature shape.
fn optional(params: &[&'static str], required_param_count: usize) -> EvalBuiltinSignatureShape {
    shape(
        required_param_count,
        params.len().saturating_sub(required_param_count),
        None,
        &[],
    )
}

/// Builds trailing-default signature shape with by-reference parameters.
fn optional_by_ref(
    params: &[&'static str],
    required_param_count: usize,
    by_ref_params: &'static [&'static str],
) -> EvalBuiltinSignatureShape {
    shape(
        required_param_count,
        params.len().saturating_sub(required_param_count),
        None,
        by_ref_params,
    )
}

/// Builds variadic signature shape.
fn variadic(
    params: &[&'static str],
    by_ref_params: &'static [&'static str],
) -> EvalBuiltinSignatureShape {
    shape(
        params.len().saturating_sub(1),
        1,
        params.last().copied(),
        by_ref_params,
    )
}

/// Builds the raw signature-shape value.
fn shape(
    required_param_count: usize,
    default_param_count: usize,
    variadic: Option<&'static str>,
    by_ref_params: &'static [&'static str],
) -> EvalBuiltinSignatureShape {
    EvalBuiltinSignatureShape {
        required_param_count,
        default_param_count,
        variadic,
        by_ref_params,
    }
}
