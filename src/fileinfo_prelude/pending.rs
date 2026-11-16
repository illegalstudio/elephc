//! Purpose:
//! Defers wrapper reads until filter creation and PHP's stream-stat gate have completed.
//!
//! Called from:
//! - The fileinfo query builder and recursive wrapper reader.
//!
//! Key details:
//! - Pending resources and prepared filters belong to one query, including during reentry.
//! - A false pending marker denotes a successfully opened wrapper without stream stat.
//! - A network marker preserves the distinction between wrapper stat and filtered socket stat.
//! - Cleanup closes the underlying wrapper before its filters, including exceptional callbacks.

use crate::parser::ast::{BinOp, CastType, Expr, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds a strict comparison without PHP coercion.
fn eq(left: Expr, right: Expr) -> Expr { e_binop(left, BinOp::StrictEq, right) }

/// Builds the warning emitted when a user wrapper has no stream_stat method.
fn missing_stat() -> Vec<Stmt> {
    vec![
        s_assign("_stat_class", e_call("elephc_runtime_stream_missing_stat_class", vec![e_call("get_resource_id", vec![e_var("_pending")])])),
        s_if(e_not(e_call("__elephc_ptr_is_null", vec![e_var("_stat_class")])), vec![
            s_assign("_stat_name", e_call("__elephc_ptr_read_string", vec![
                e_call("elephc_runtime_stream_stat_name", vec![e_var("_stat_class")]),
                e_call("elephc_runtime_stream_stat_name_length", vec![e_var("_stat_class")]),
            ])),
            s_expr(e_call("__elephc_builtin_warning", vec![e_binop(
                e_binop(e_var("_function"), BinOp::Concat, e_str("(): ")), BinOp::Concat,
                e_binop(e_var("_stat_name"), BinOp::Concat, e_str("::stream_stat is not implemented!")),
            )])),
            s_return(e_bool(false)),
        ], vec![], None),
    ]
}

/// Resolves stream metadata before any user-wrapper read or byte-filter processing.
fn read_pending() -> Vec<Stmt> {
    let mut body = missing_stat();
    body.extend([
        s_typed_assign(t_mixed(), "_stat", e_call("fstat", vec![e_var("_pending")])),
        s_if(e_not(e_call("is_array", vec![e_var("_stat")])), vec![s_expr(e_call("unset", vec![e_var("_stat")])), s_return(e_bool(false))], vec![], None),
        s_assign("_mode", e_cast(CastType::Int, e_null_coalesce(e_index(e_var("_stat"), e_str("mode")), e_int(0)))),
        s_expr(e_call("unset", vec![e_var("_stat")])),
        s_if(e_binop(e_binop(e_var("_mode"), BinOp::BitAnd, e_int(0o040000)), BinOp::NotEq, e_int(0)),
            vec![s_return(e_str("directory"))], vec![], None),
        s_if(e_not(e_method_call(e_this(), "__elephc_magic_stat", vec![e_var("_pending"), e_var("_function")])), vec![s_return(e_bool(false))], vec![], None),
    ]);
    body.push(super::stream_magic::read_phase(vec![
        s_assign("_data", e_call("stream_get_contents", vec![e_var("_pending"), e_int(7340032)])),
        s_if(eq(e_var("_data"), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
        s_assign("_cast_filtered", e_bool(false)),
        s_foreach(e_var("_stages"), None, "_prepared", vec![
            s_if(e_binop(e_var("_prepared"), BinOp::StrictNotEq, e_null()), vec![s_assign("_cast_filtered", e_bool(true))], vec![], None),
            s_if(e_call("is_resource", vec![e_var("_prepared")]), vec![
                s_expr(e_call("fwrite", vec![e_var("_prepared"), e_cast(CastType::String, e_var("_data"))])),
                s_expr(e_call("rewind", vec![e_var("_prepared")])),
                s_assign("_data", e_call("stream_get_contents", vec![e_var("_prepared"), e_int(7340032)])),
            ], vec![], Some(vec![s_if(e_call("is_string", vec![e_var("_prepared")]), vec![
                s_assign("_data", e_method_call(e_this(), "__elephc_filter_bytes", vec![
                    e_cast(CastType::String, e_var("_data")), e_cast(CastType::String, e_var("_prepared")), e_var("_function"), e_bool(false),
                ])),
            ], vec![], None)])),
            s_if(eq(e_var("_data"), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
        ]),
    ], e_var("_function")));
    body.push(s_if(e_not(e_var("_cast_filtered")), vec![s_expr(e_method_call(e_this(), "__elephc_magic_cast", vec![e_var("_pending"), e_var("_function")]))], vec![], None));
    body
}

/// Owns the query's deferred streams and enforces PHP's stat-before-identification rule.
pub(super) fn query() -> Vec<Stmt> {
    vec![
        s_assign("_stream_directory", e_bool(false)), s_typed_assign(t_mixed(), "_pending", e_null()), s_typed_assign(TypeExpr::Array(Box::new(t_mixed())), "_stages", e_array(vec![])), s_assign("_query_unwind", e_bool(false)),
        s_try(vec![
            s_typed_assign(t_union(vec![TypeExpr::Str, TypeExpr::False]), "_data", e_method_call(e_this(), "__elephc_read_stream", vec![
                e_var("filename"), e_var("context"), e_var("_function"), e_bool(true), e_var("_stream_directory"), e_var("_pending"), e_var("_stages"),
            ])),
            s_if(eq(e_var("_data"), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
            s_if(eq(e_var("_pending"), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
            s_if(eq(e_var("_pending"), e_int(-2)), vec![s_return(e_ternary(e_var("_stream_directory"), e_str("directory"), e_bool(false)))], vec![], None),
            s_if(e_call("is_resource", vec![e_var("_pending")]), read_pending(), vec![], None),
            s_if(e_var("_stream_directory"), vec![s_return(e_str("directory"))], vec![], None),
            s_return(e_method_call(e_this(), "__elephc_buffer", vec![e_var("_function"), e_cast(CastType::String, e_var("_data")), e_int(0)])),
        ], vec![(vec!["Throwable"], Some("_query_error"), vec![s_assign("_query_unwind", e_bool(true)), s_throw(e_var("_query_error"))])],
            Some(vec![s_expr(e_method_call(e_this(), "__elephc_close_streams", vec![e_var("_pending"), e_var("_stages"), e_var("_query_unwind")]))])),
    ]
}

/// Runs user filter creation before stream stat, retaining its temporary stream for later bytes.
pub(super) fn prepare_method() -> MethodBuilder {
    let mut body = Vec::new();
    for name in ["string.toupper", "string.tolower", "string.rot13", "string.strip_tags", "dechunk",
        "convert.base64-encode", "convert.base64-decode", "convert.quoted-printable-encode",
        "convert.quoted-printable-decode", "bzip2.decompress", "zlib.inflate", "zlib.deflate"] {
        body.push(s_if(eq(e_var("name"), e_str(name)), vec![s_return(e_ternary(e_var("write"), e_array(vec![e_var("name")]), e_var("name")))], vec![], None));
    }
    body.extend([
        s_assign("_stage_stream", e_call("fopen", vec![e_str("php://temp"), e_str("w+b")])),
        s_if(eq(e_var("_stage_stream"), e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
        s_assign("_transferred", e_bool(false)), s_assign("_prepare_unwind", e_bool(false)),
        s_try(vec![
            s_assign("_attached", e_call("stream_filter_append", vec![e_var("_stage_stream"), e_var("name"), e_ternary(e_var("write"), e_const("STREAM_FILTER_WRITE"), e_const("STREAM_FILTER_READ"))])),
            s_if(eq(e_var("_attached"), e_bool(false)), vec![
                s_expr(e_call("__elephc_builtin_warning", vec![e_binop(e_binop(e_var("_function"), BinOp::Concat, e_str("(): Unable to locate filter \"")),
                    BinOp::Concat, e_binop(e_var("name"), BinOp::Concat, e_str("\"")))])),
                s_expr(e_call("__elephc_builtin_warning", vec![e_binop(e_binop(e_var("_function"), BinOp::Concat, e_str("(): Unable to create filter (")),
                    BinOp::Concat, e_binop(e_var("name"), BinOp::Concat, e_str(")")))])),
                s_return(e_null()),
            ], vec![], None),
            s_assign("_transferred", e_bool(true)), s_return(e_ternary(e_var("write"), e_array(vec![e_var("_stage_stream")]), e_var("_stage_stream"))),
        ], vec![(vec!["Throwable"], Some("_prepare_error"), vec![s_assign("_prepare_unwind", e_bool(true)), s_throw(e_var("_prepare_error"))])],
            Some(vec![s_if(e_not(e_var("_transferred")), vec![s_expr(e_method_call(e_this(), "__elephc_close_streams", vec![e_var("_stage_stream"), e_array(vec![]), e_var("_prepare_unwind")]))], vec![], None)])),
    ]);
    method("__elephc_prepare_filter").private().param("name", TypeExpr::Str).param("_function", TypeExpr::Str).param_default("write", TypeExpr::Bool, e_bool(false)).returns(t_mixed()).body(body)
}

/// Closes every owned stream even if one user close callback throws, then rethrows its first error.
pub(super) fn cleanup_method() -> MethodBuilder {
    let close = vec![s_if(e_call("is_resource", vec![e_var("_owned")]), vec![
        s_try(vec![s_expr(e_call("fclose", vec![e_var("_owned")]))], vec![(vec!["Throwable"], Some("_close_error"), vec![
            s_if(eq(e_var("_close_failure"), e_null()), vec![s_assign("_close_failure", e_var("_close_error"))], vec![], None),
            s_expr(e_call("elephc_runtime_stream_close_mode", vec![e_int(1)])),
        ])], None),
    ], vec![], None)];
    let mut body = vec![s_typed_assign(t_nullable(t_class("Throwable")), "_close_failure", e_null()), s_assign("_owned", e_var("pending"))];
    body.extend(close.clone());
    let mut stage_close = vec![s_if(e_call("is_array", vec![e_var("_owned")]), vec![s_assign("_owned", e_index(e_var("_owned"), e_int(0)))], vec![], None)];
    stage_close.extend(close);
    body.push(s_foreach(e_var("stages"), None, "_owned", stage_close));
    body.push(s_if(e_binop(e_var("_close_failure"), BinOp::StrictNotEq, e_null()), vec![s_throw(e_var("_close_failure"))], vec![], None));
    let scoped = vec![
        s_assign("_previous_mode", e_call("elephc_runtime_stream_close_mode", vec![e_int(-1)])),
        s_expr(e_call("elephc_runtime_stream_close_mode", vec![e_cast(CastType::Int,
            e_binop(e_var("silent"), BinOp::Or, e_binop(e_var("_previous_mode"), BinOp::NotEq, e_int(0))))])),
        s_try(body, vec![], Some(vec![s_expr(e_call("elephc_runtime_stream_close_mode", vec![e_var("_previous_mode")]))])),
    ];
    method("__elephc_close_streams").private().param("pending", t_mixed()).param("stages", TypeExpr::Array(Box::new(t_mixed()))).param("silent", TypeExpr::Bool).body(scoped)
}
