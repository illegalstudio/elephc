//! Purpose:
//! Preserves PHP magic_stream's stat and cast callbacks around stream byte identification.
//!
//! Called from:
//! - Deferred URL reads and the already-open mime_content_type resource path.
//!
//! Key details:
//! - The identification stat is independent of the URL directory/false gate.
//! - Successfully attached read or write filters disable the inner cast callback.
//! - A cast never replaces the bytes that the wrapper already supplied.

use crate::parser::ast::{BinOp, Expr, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Joins a public function frame and the diagnostic emitted by its stream callback.
fn message(tail: Expr) -> Expr {
    e_binop(e_binop(e_var("_function"), BinOp::Concat, e_str("(): ")), BinOp::Concat, tail)
}

/// Copies a class-name metadata span without assuming NUL termination.
fn class_name(row: &str) -> Expr {
    e_call("__elephc_ptr_read_string", vec![
        e_call("elephc_runtime_stream_stat_name", vec![e_var(row)]),
        e_call("elephc_runtime_stream_stat_name_length", vec![e_var(row)]),
    ])
}

/// Emits a named missing-method warning while allowing identification to continue.
fn missing_stat() -> Vec<Stmt> {
    vec![
        s_assign("_stat_class", e_call("elephc_runtime_stream_missing_stat_class", vec![e_call("get_resource_id", vec![e_var("stream")])])),
        s_if(e_not(e_call("__elephc_ptr_is_null", vec![e_var("_stat_class")])), vec![
            super::build::warning(message(e_binop(class_name("_stat_class"), BinOp::Concat, e_str("::stream_stat is not implemented!")))),
        ], vec![], None),
    ]
}

/// Performs magic_stream's extra stat, honoring the active libmagic MAGIC_ERROR bit.
pub(super) fn stat_method() -> MethodBuilder {
    let mut stat = missing_stat();
    stat.extend([
        s_typed_assign(t_mixed(), "_magic_stat", e_call("fstat", vec![e_var("stream")])),
        s_assign("_stat_ok", e_call("is_array", vec![e_var("_magic_stat")])),
        s_expr(e_call("unset", vec![e_var("_magic_stat")])),
    ]);
    let mut body = vec![s_try(stat, vec![(vec!["Throwable"], Some("_stat_error"), vec![
        s_expr(e_call("elephc_runtime_stream_exception_warning", vec![e_binop(e_str("Warning: "), BinOp::Concat,
            message(e_str("Failed identify data 0:cannot read `(null)'\n")))])),
        s_throw(e_var("_stat_error")),
    ])], None)];
    body.extend([
        s_if(e_binop(e_not(e_var("_stat_ok")), BinOp::And,
            e_binop(e_binop(e_call("elephc_fileinfo_get_flags", vec![e_this_prop("__elephc_handle")]), BinOp::BitAnd, e_int(512)), BinOp::NotEq, e_int(0))), vec![
            super::build::warning(message(e_str("Failed identify data 0:cannot stat `(null)'"))),
            s_return(e_bool(false)),
        ], vec![], None),
        s_return(e_bool(true)),
    ]);
    method("__elephc_magic_stat").final_().param("stream", t_mixed()).param("_function", TypeExpr::Str).returns(TypeExpr::Bool).body(body)
}

/// Invokes stream_cast(0), framing recoverable warnings and invalid-resource errors as PHP does.
pub(super) fn cast_method() -> MethodBuilder {
    let mut body = vec![
        s_assign("_cast_id", e_call("get_resource_id", vec![e_var("stream")])),
        s_assign("_cast_class", e_call("elephc_runtime_stream_class", vec![e_var("_cast_id")])),
        s_if(e_call("__elephc_ptr_is_null", vec![e_var("_cast_class")]), vec![s_return_void()], vec![], None),
        s_assign("_cast_name", class_name("_cast_class")),
        s_assign("_cast_result", e_call("elephc_runtime_fileinfo_cast", vec![e_var("_cast_id")])),
    ];
    let mut work = vec![s_assign("_cast_status", e_call("elephc_runtime_fileinfo_cast_status", vec![e_var("_cast_result")]))];
    for (status, diagnostic) in [(1, "::stream_cast is not implemented!"), (2, "::stream_cast must return a stream resource"), (3, "::stream_cast must not return itself")] {
        let diagnostic = message(e_binop(e_var("_cast_name"), BinOp::Concat, e_str(diagnostic)));
        let mut failure = if status == 2 {
            vec![s_expr(e_call("elephc_runtime_stream_exception_warning", vec![e_binop(e_str("Warning: "), BinOp::Concat,
                e_binop(diagnostic, BinOp::Concat, e_str("\n")))]))]
        } else { vec![super::build::warning(diagnostic)] };
        if status == 2 {
            failure.push(s_throw(e_new_fq("TypeError", vec![message(e_str("supplied argument is not a valid stream resource"))])));
        }
        work.push(s_if(e_binop(e_var("_cast_status"), BinOp::Eq, e_int(status)), failure, vec![], None));
    }
    body.push(s_try(work, vec![], Some(vec![s_expr(e_call("elephc_runtime_fileinfo_cast_release", vec![e_var("_cast_result")]))])));
    method("__elephc_magic_cast").final_().param("stream", t_mixed()).param("_function", TypeExpr::Str).returns(TypeExpr::Void).body(body)
}

/// Preserves magic_stream's cannot-read warning when a read or read-filter callback throws.
pub(super) fn read_phase(body: Vec<Stmt>, function: Expr) -> Stmt {
    s_try(body, vec![(vec!["Throwable"], Some("_read_error"), vec![
        s_expr(e_call("elephc_runtime_stream_exception_warning", vec![e_binop(e_str("Warning: "), BinOp::Concat,
            e_binop(function, BinOp::Concat, e_str("(): Failed identify data 0:cannot read `(null)'\n")))])),
        s_throw(e_var("_read_error")),
    ])], None)
}

/// Reads an existing resource without applying the URL-only false/directory stat gate.
pub(super) fn resource_query() -> Vec<Stmt> {
    vec![
        s_if(e_binop(e_call("get_resource_type", vec![e_var("filename")]), BinOp::StrictNotEq, e_str("stream")), vec![
            s_throw(e_new_fq("TypeError", vec![e_str("mime_content_type(): supplied resource is not a valid stream resource")])),
        ], vec![], None),
        s_assign("_position", e_call("ftell", vec![e_var("filename")])),
        s_expr(e_call("fseek", vec![e_var("filename"), e_int(0)])),
        s_assign("_resource_unwind", e_bool(false)),
        s_try(vec![
            s_if(e_not(e_method_call(e_var("_finfo"), "__elephc_magic_stat", vec![e_var("filename"), e_str("mime_content_type")])), vec![s_return(e_bool(false))], vec![], None),
            read_phase(vec![
                s_assign("_data", e_call("stream_get_contents", vec![e_var("filename"), e_int(7340032)])),
                s_if(e_binop(e_var("_data"), BinOp::StrictEq, e_bool(false)), vec![s_return(e_bool(false))], vec![], None),
            ], e_str("mime_content_type")),
            s_expr(e_method_call(e_var("_finfo"), "__elephc_magic_cast", vec![e_var("filename"), e_str("mime_content_type")])),
            s_return(e_method_call(e_var("_finfo"), "__elephc_buffer", vec![e_str("mime_content_type"), e_var("_data")])),
        ], vec![(vec!["Throwable"], Some("_resource_error"), vec![s_assign("_resource_unwind", e_bool(true)), s_throw(e_var("_resource_error"))])],
            Some(vec![s_if(e_not(e_var("_resource_unwind")), vec![s_expr(e_call("fseek", vec![e_var("filename"), e_var("_position")]))], vec![], None)])),
    ]
}
