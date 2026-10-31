//! Purpose:
//! Reads runtime-built fileinfo data URLs without relying on literal-only stream lowering.
//!
//! Called from:
//! - The synthetic finfo file query builder, after filename and context validation.
//!
//! Key details:
//! - Data URLs preserve binary payloads and follow PHP's base64/form URL decoding.
//! - Local file URLs use the same descriptor path and native flags as local filenames.

use crate::parser::ast::{BinOp, CastType, Expr, Stmt};
use crate::synthetic_class::*;

/// Emits the recoverable warning used for malformed or non-local wrapper inputs.
fn invalid(message: &str) -> Vec<Stmt> {
    failure(e_str(message))
}

/// Emits a wrapper failure with the original URL and public fileinfo call name.
pub(super) fn failure(reason: Expr) -> Vec<Stmt> {
    let mut message = e_binop(e_var("_function"), BinOp::Concat, e_str("("));
    message = e_binop(message, BinOp::Concat, e_var("filename"));
    message = e_binop(message, BinOp::Concat, e_str("): Failed to open stream: "));
    vec![s_if(e_var("_report_errors"), vec![super::build::warning(e_binop(message, BinOp::Concat, reason))], vec![], None), s_return(e_bool(false))]
}

/// Decodes a data URL into bytes for the runtime stream reader.
pub(super) fn data_uri() -> Stmt {
    s_if(e_call("str_starts_with", vec![e_var("filename"), e_str("data:")]), vec![
        s_assign("_uri_start", e_ternary(e_call("str_starts_with", vec![e_var("filename"), e_str("data://")]), e_int(7), e_int(5))),
        s_assign("_uri_comma", e_call("strpos", vec![e_var("filename"), e_str(",")])),
        s_if(e_binop(e_var("_uri_comma"), BinOp::StrictEq, e_bool(false)), invalid("rfc2397: no comma in URL"), vec![], None),
        s_assign("_uri_meta", e_call("substr", vec![e_var("filename"), e_var("_uri_start"), e_binop(e_var("_uri_comma"), BinOp::Sub, e_var("_uri_start"))])),
        s_assign("_uri_base64", e_bool(false)),
        metadata(),
        s_assign("_uri_payload", e_call("substr", vec![e_var("filename"), e_binop(e_var("_uri_comma"), BinOp::Add, e_int(1))])),
        s_if(e_var("_uri_base64"), vec![
            s_assign("_uri_data", e_call("base64_decode", vec![e_var("_uri_payload"), e_bool(true)])),
        ], vec![], Some(vec![s_assign("_uri_data", e_call("urldecode", vec![e_var("_uri_payload")]))])),
        s_if(e_binop(e_var("_uri_data"), BinOp::StrictEq, e_bool(false)), invalid("rfc2397: unable to decode"), vec![], None),
        s_return(e_call("substr", vec![e_var("_uri_data"), e_int(0), e_int(7340032)])),
    ], vec![], Some(vec![s_if(e_call("str_starts_with", vec![e_call("strtolower", vec![e_var("filename")]), e_str("data://")]),
        invalid("operation failed"), vec![], None)]))
}

/// Validates PHP's media type and parameter grammar, including the exact final base64 token.
fn metadata() -> Stmt {
    s_if(e_binop(e_var("_uri_meta"), BinOp::StrictNotEq, e_str("")), vec![
        s_assign("_uri_semi", e_call("strpos", vec![e_var("_uri_meta"), e_str(";")])),
        s_assign("_uri_slash", e_call("strpos", vec![e_var("_uri_meta"), e_str("/")])),
        s_if(e_binop(e_var("_uri_semi"), BinOp::StrictEq, e_bool(false)), vec![
            s_if(e_binop(e_var("_uri_slash"), BinOp::StrictEq, e_bool(false)), invalid("rfc2397: illegal media type"), vec![], None),
        ], vec![], Some(vec![
            s_if(e_binop(e_binop(
                e_binop(e_var("_uri_slash"), BinOp::StrictEq, e_bool(false)), BinOp::Or,
                e_binop(e_var("_uri_slash"), BinOp::Gt, e_var("_uri_semi")),
            ), BinOp::And, e_binop(e_var("_uri_meta"), BinOp::StrictNotEq, e_str(";base64"))), invalid("rfc2397: illegal media type"), vec![], None),
            s_assign("_uri_parameters", e_call("substr", vec![e_var("_uri_meta"), e_binop(e_var("_uri_semi"), BinOp::Add, e_int(1))])),
            s_foreach(e_call("explode", vec![e_str(";"), e_var("_uri_parameters")]), None, "_uri_parameter", vec![
                s_if(e_var("_uri_base64"), invalid("rfc2397: illegal parameter"), vec![], None),
                s_if(e_binop(e_call("strpos", vec![e_var("_uri_parameter"), e_str("=")]), BinOp::StrictEq, e_bool(false)), vec![
                    s_if(e_binop(e_var("_uri_parameter"), BinOp::StrictNotEq, e_str("base64")), invalid("rfc2397: illegal parameter"), vec![], None),
                    s_assign("_uri_base64", e_bool(true)),
                ], vec![], None),
            ]),
        ])),
    ], vec![], None)
}

/// Normalizes local file URLs before native descriptor and directory handling.
pub(super) fn file_uri() -> Stmt {
    s_if(e_call("str_starts_with", vec![e_call("strtolower", vec![e_var("filename")]), e_str("file://")]), vec![
        s_assign("filename", e_call("substr", vec![e_var("filename"), e_int(7)])),
        s_if(e_binop(e_var("filename"), BinOp::StrictEq, e_str("")), vec![s_assign("filename", e_str("/"))], vec![], None),
        s_if(e_call("str_starts_with", vec![e_call("strtolower", vec![e_var("filename")]), e_str("localhost/")]), vec![
            s_assign("filename", e_call("substr", vec![e_var("filename"), e_int(9)])),
        ], vec![], None),
        s_if(e_not(e_call("str_starts_with", vec![e_var("filename"), e_str("/")])), vec![s_return(e_bool(false))], vec![], None),
    ], vec![], None)
}

/// Uses the runtime network reader and the existing stream-context option store.
pub(super) fn network_uri() -> Stmt {
    let mut network = e_bool(false);
    for scheme in ["http://", "https://", "ftp://", "ftps://"] {
        network = e_binop(network, BinOp::Or, e_call("str_starts_with", vec![e_call("strtolower", vec![e_var("filename")]), e_str(scheme)]));
    }
    s_if(network, vec![
        s_expr(e_call("elephc_runtime_stream_reset_error", vec![])),
        s_assign("_url_colon", e_cast(CastType::Int, e_call("strpos", vec![e_var("filename"), e_str(":")]))),
        s_assign("_url_name", e_binop(e_call("strtolower", vec![e_call("substr", vec![e_var("filename"), e_int(0), e_var("_url_colon")])]),
            BinOp::Concat, e_call("substr", vec![e_var("filename"), e_var("_url_colon")]))),
        // Context resources share the stream layer's option store. Passing literal null
        // selects that store without hitting file_get_contents' dynamic-context ABI guard.
        s_assign("_url_data", e_call("file_get_contents", vec![e_var("_url_name"), e_bool(false), e_null(), e_int(0), e_int(7340032)])),
        s_if(e_binop(e_var("_url_data"), BinOp::StrictEq, e_bool(false)), failure(e_call("elephc_fileinfo_network_error", vec![
            e_call("elephc_runtime_stream_error_code", vec![]), e_call("elephc_runtime_stream_tls_error", vec![]),
        ])), vec![], None),
        // Direct network wrapper stat fails; a filter wrapper can expose the socket's stat instead.
        s_assign("_pending", e_int(-2)), s_return(e_str("")),
    ], vec![], None)
}
