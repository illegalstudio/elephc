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

use super::{
    eval_builtin_param_names, eval_declared_builtin_default_value, eval_declared_builtin_spec,
};

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
    if let Some(spec) = eval_declared_builtin_spec(name) {
        return Some(EvalBuiltinSignatureShape {
            required_param_count: spec.required_param_count(),
            default_param_count: spec.default_param_count(),
            variadic: spec.variadic,
            by_ref_params: spec.by_ref_param_names(),
        });
    }

    let params = eval_builtin_param_names(name)?;
    Some(match name {
        "isset" | "unset" => variadic(params, &[]),
        "class_alias" => optional(params, 2),
        "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
        | "class_implements" | "class_parents" | "class_uses" => optional(params, 1),
        "get_class" | "get_parent_class" => optional(params, 0),
        "is_a" | "is_subclass_of" => optional(params, 2),

        "is_callable" => optional_by_ref(params, 1, &["callable_name"]),
        "readline" => optional(params, 0),

        "fprintf" | "fscanf" => variadic(params, &[]),

        "fopen" | "fputcsv" => optional(params, 2),
        "flock" => optional_by_ref(params, 2, &["would_block"]),
        "fgetcsv" => optional(params, 1),
        "stream_socket_accept" => optional_by_ref(params, 1, &["peer_name"]),
        "fsockopen" | "pfsockopen" => {
            optional_by_ref(params, 2, &["error_code", "error_message"])
        }
        "stream_wrapper_register" | "stream_socket_enable_crypto" => optional(params, 2),
        "stream_context_create" | "stream_context_get_default" => optional(params, 0),
        "stream_context_set_option" => optional(params, 2),
        "stream_socket_sendto" | "stream_filter_append" | "stream_filter_prepend" => {
            optional(params, 2)
        }
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
    if let Some(default_value) = eval_declared_builtin_default_value(name, param_index) {
        return Some(default_value);
    }

    use EvalBuiltinDefaultValue::*;

    Some(match (name, param_index) {
        ("class_alias", 2) => Bool(true),
        (
            "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
            | "class_implements" | "class_parents" | "class_uses",
            1,
        ) => Bool(true),
        ("get_class" | "get_parent_class", 0) => Null,
        ("is_a", 2) => Bool(false),
        ("is_subclass_of", 2) => Bool(true),

        ("is_callable", 1) => Bool(false),
        ("is_callable", 2) => Null,
        ("readline", 0) => Null,
        ("fopen", 2) => Bool(false),
        ("fopen", 3) => Null,
        ("flock", 2) => Null,
        ("fgetcsv", 1) => Null,
        ("fgetcsv", 2) => String(","),
        ("fputcsv", 2) => String(","),
        ("fputcsv", 3) => String("\""),
        ("stream_socket_accept", 1 | 2) => Null,
        ("fsockopen" | "pfsockopen", 2 | 3 | 4) => Null,
        ("stream_wrapper_register", 2) => Int(0),
        ("stream_socket_enable_crypto", 2 | 3) => Null,
        ("stream_context_create", 0 | 1) => Null,
        ("stream_context_get_default", 0) => Null,
        ("stream_context_set_option", 2 | 3) => Null,
        ("stream_select", 4) => Int(0),
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
