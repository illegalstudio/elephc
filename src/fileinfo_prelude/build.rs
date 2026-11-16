//! Purpose:
//! Builds the fileinfo PHP surface as synthetic AST declarations.
//!
//! Called from:
//! - fileinfo_declarations before name resolution.
//!
//! Key details:
//! - Objects exclusively own native handles; finfo_close is a PHP 8 no-op.
//! - Query overrides are temporary. Streams use existing PHP stream wrappers.

use crate::parser::ast::{BinOp, CastType, CType, Expr, ExprKind, Program, Stmt, StmtKind, TypeExpr};
use crate::php_version::PhpVersion;
use crate::span::Span;
use crate::synthetic_class::*;

/// Returns a comparison with strict identity semantics.
fn eq(left: Expr, right: Expr) -> Expr { e_binop(left, BinOp::StrictEq, right) }
/// Returns a concatenation of two PHP strings.
fn concat(left: Expr, right: Expr) -> Expr { e_binop(left, BinOp::Concat, right) }
/// Returns the exact string-or-false PHP result declaration.
fn result_type() -> TypeExpr { t_union(vec![TypeExpr::Str, TypeExpr::False]) }
/// Throws the indicated PHP exception when a condition holds.
fn reject(condition: Expr, class: &str, message: &str) -> Stmt {
    s_if(condition, vec![s_throw(e_new_fq(class, vec![e_str(message)]))], vec![], None)
}
/// Emits a PHP warning and returns false for a recoverable native failure.
fn failure(file: bool) -> Vec<Stmt> {
    query_failure(file)
}
/// Requires a valid object handle before each operation.
fn require_handle() -> Vec<Stmt> {
    vec![s_assign("_raw", e_this_prop("__elephc_handle")), reject(eq(e_var("_raw"), e_int(0)), "Error", "Invalid finfo object")]
}
/// Validates the nullable resource accepted by fileinfo query methods.
fn context_check() -> Stmt {
    s_if(non_resource_context(),
        context_type_error(e_var("_function")), vec![], None)
}

/// Emits PHP 8.5 deprecations while leaving older selected profiles unchanged.
fn deprecated(message: &str, supplied_context: Option<i64>) -> Stmt {
    let mut condition = e_binop(e_const("PHP_VERSION_ID"), BinOp::GtEq, e_int(80500));
    if let Some(count) = supplied_context {
        condition = e_binop(condition, BinOp::And, e_binop(e_call("func_num_args", vec![]), BinOp::Gt, e_int(count)));
    }
    s_if(condition, vec![s_expr(e_call("__elephc_builtin_deprecated", vec![e_str(message)]))], vec![], None)
}

/// Builds one query method, preserving bytes and PHP stream-context behavior.
fn query_engine(file: bool) -> crate::synthetic_class::MethodBuilder {
    let (name, parameter) = if file { ("file", "filename") } else { ("buffer", "string") };
    let mut body = require_handle();
    body.push(context_check());
    if file {
        body.extend([
            s_if(e_binop(e_binop(e_var("context"), BinOp::StrictNotEq, e_null()), BinOp::And,
                e_binop(e_call("get_resource_type", vec![e_var("context")]), BinOp::StrictNotEq, e_str("stream-context"))), vec![s_throw(e_new_fq("TypeError", vec![concat(e_var("_function"), e_str("(): supplied resource is not a valid Stream-Context resource"))]))], vec![], None),
            filename_check(false),
            filename_check(true),
            s_assign("_source_filename", e_var(parameter)),
            super::streams::file_uri(),
            // Directory identity follows the opened descriptor through nested wrappers.
            // The output flag belongs to this call, so callbacks can reenter safely.
            // Native directories never enter libmagic's buffer identification path.
            // Filter attachment still runs before this result is returned.
            s_if(e_method_call(e_this(), "__elephc_is_stream", vec![e_var(parameter), e_var("_function")]), stream_query(), vec![], None),
            s_assign("_result", e_call("elephc_fileinfo_file", vec![e_var("_raw"), e_var(parameter), e_int(0)])),
        ]);
    } else {
        body.push(s_assign("_result", e_call("elephc_fileinfo_buffer", vec![e_var("_raw"), e_var(parameter), e_call("strlen", vec![e_var(parameter)]), e_int(0)])));
    }
    body.push(s_if(eq(e_call("elephc_fileinfo_ok", vec![]), e_int(0)), failure(file), vec![], None));
    body.push(s_return(e_var("_result")));
    method(&format!("__elephc_{name}")).final_().param("_function", TypeExpr::Str).param(parameter, TypeExpr::Str)
        .param_default("flags", TypeExpr::Int, e_int(0))
        .param_default("context", t_mixed(), e_null()).returns(result_type()).body(with_query_flags(body))
}

/// Public method dispatch remains overridable; procedural calls use the final engine helper.
fn query_method(file: bool) -> crate::synthetic_class::MethodBuilder {
    let (name, parameter) = if file { ("file", "filename") } else { ("buffer", "string") };
    let mut body = Vec::new();
    if !file {
        body.push(buffer_context("finfo::buffer", 2));
    }
    body.push(s_return(e_method_call(e_this(), &format!("__elephc_{name}"), vec![
        e_str(&format!("finfo::{name}")), e_var(parameter), e_var("flags"), e_var("context"),
    ])));
    method(name).param(parameter, TypeExpr::Str)
        .param_default("flags", TypeExpr::Int, e_int(0))
        .param_default("context", t_mixed(), e_null()).returns(result_type())
.body(body)
}

/// Builds the owning class, including PHP's clone and serialization restrictions.
fn finfo_class() -> Stmt {
    let mut constructor = vec![
        s_assign("_database", e_cast(CastType::String, e_var("magic_database"))),
        reject(e_binop(e_call("strpos", vec![e_var("_database"), e_str("\0")]), BinOp::StrictNotEq, e_bool(false)), "ValueError", "finfo::__construct(): Argument #2 ($magic_database) must not contain any null bytes"),
        s_prop_assign(e_this(), "__elephc_handle", e_int(0)),
        s_prop_assign(e_this(), "__elephc_owner", e_null()),
        s_assign("_raw", e_call("elephc_fileinfo_open", vec![e_var("flags"), e_var("_database")])),
    ];
    constructor.push(s_if(eq(e_var("_raw"), e_int(0)), vec![s_throw(e_new_fq("Exception", vec![concat(e_str("finfo::__construct(): "), e_call("elephc_fileinfo_error", vec![]))]))], vec![], None));
    constructor.push(s_prop_assign(e_this(), "__elephc_owner", e_new_fq("__ElephcFinfoHandle", vec![e_var("_raw")])));
    constructor.push(s_prop_assign(e_this(), "__elephc_handle", e_var("_raw")));
    let mut set_flags = require_handle();
    set_flags.extend([s_expr(e_call("elephc_fileinfo_set_flags", vec![e_var("_raw"), e_var("flags")])), s_return(e_bool(true))]);
    class("finfo")
        .private_prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .private_prop("__elephc_owner", t_nullable(t_class("__ElephcFinfoHandle")), Some(e_null()))
        .method(method("__construct").param_default("flags", TypeExpr::Int, e_int(0))
            .param_default("magic_database", t_nullable(TypeExpr::Str), e_null()).body(constructor))
        .method(query_method(false)).method(query_method(true))
        .method(query_engine(false)).method(query_engine(true))
        .method(super::wrappers::read_method()).method(super::wrappers::filter_method()).method(super::wrappers::routing_method()).method(super::pending::prepare_method()).method(super::pending::cleanup_method()).method(super::stream_magic::stat_method()).method(super::stream_magic::cast_method())
        .method(method("set_flags").param("flags", TypeExpr::Int).returns(TypeExpr::Bool)
            .returning(e_method_call(e_this(), "__elephc_set_flags", vec![e_var("flags")])))
        .method(method("__elephc_set_flags").final_().param("flags", TypeExpr::Int).returns(TypeExpr::Bool).body(set_flags))
        .method(method("__debugInfo").returns(t_array()).returning(e_array(vec![])))
        .method(method("__clone").returns(TypeExpr::Void).body(vec![
            s_prop_assign(e_this(), "__elephc_handle", e_int(0)),
            s_prop_assign(e_this(), "__elephc_owner", e_null()),
            s_throw(e_new_fq("Error", vec![concat(e_str("Trying to clone an uncloneable object of class "), e_call("get_class", vec![e_this()]))])),
        ]))
        .method(method("__serialize").returns(t_array()).body(vec![s_throw(e_new_fq("Exception", vec![e_str("Serialization of 'finfo' is not allowed")]))]))
        .method(method("__unserialize").param("data", t_array()).returns(TypeExpr::Void).body(vec![s_throw(e_new_fq("Exception", vec![e_str("Unserialization of 'finfo' is not allowed")]))]))
        .build()
}

/// Keeps native cleanup independent of a finfo subclass's user-defined destructor.
fn handle_owner() -> Stmt {
    class("__ElephcFinfoHandle").final_()
        .private_prop("handle", TypeExpr::Int, Some(e_int(0)))
        .method(method("__construct").param("handle", TypeExpr::Int).body(vec![s_prop_assign(e_this(), "handle", e_var("handle"))]))
        .method(method("__destruct").body(vec![
            s_assign("_handle", e_this_prop("handle")),
            s_prop_assign(e_this(), "handle", e_int(0)),
            s_expr(e_call("elephc_fileinfo_free", vec![e_var("_handle")])),
        ])).build()
}

/// Builds procedural wrappers with the shared contract's PHP parameter names.
fn functions(php_version: PhpVersion) -> Program {
    let mut out = vec![
        function("finfo_open").param_default("flags", TypeExpr::Int, e_int(0))
            .param_default("magic_database", t_nullable(TypeExpr::Str), e_null())
            .returns(t_union(vec![t_class("finfo"), TypeExpr::False]))
            .body(vec![
                reject(e_binop(e_call("strpos", vec![e_cast(CastType::String, e_var("magic_database")), e_str("\0")]), BinOp::StrictNotEq, e_bool(false)), "ValueError", "finfo_open(): Argument #2 ($magic_database) must not contain any null bytes"),
                s_try(vec![s_return(e_new_fq("finfo", vec![e_var("flags"), e_var("magic_database")]))],
                vec![(vec!["Exception"], Some("_error"), vec![
                    s_expr(e_call("__elephc_builtin_warning", vec![concat(e_str("finfo_open(): "), e_call("substr", vec![e_method_call(e_var("_error"), "getMessage", vec![]), e_int("finfo::__construct(): ".len() as i64)]))])),
                    s_return(e_bool(false)),
                ])], None)]).build(),
        function("finfo_close").param("finfo", t_class("finfo")).returns(TypeExpr::Bool).body(vec![
            deprecated("Function finfo_close() is deprecated since 8.5, as finfo objects are freed automatically", None),
            s_return(e_bool(true)),
        ]).build(),
        function("finfo_set_flags").param("finfo", t_class("finfo")).param("flags", TypeExpr::Int)
            .returns(TypeExpr::Bool).returning(e_method_call(e_var("finfo"), "__elephc_set_flags", vec![e_var("flags")])).build(),
    ];
    out.push(function("finfo_buffer").param("finfo", t_class("finfo")).param("string", TypeExpr::Str)
        .param_default("flags", TypeExpr::Int, e_int(0)).param_default("context", t_mixed(), e_null())
        .returns(result_type()).body(vec![
            buffer_context("finfo_buffer", 3),
            s_return(e_method_call(e_var("finfo"), "__elephc_buffer", vec![e_str("finfo_buffer"), e_var("string"), e_var("flags"), e_var("context")])),
        ]).build());
    out.push(function("finfo_file").param("finfo", t_class("finfo")).param("filename", TypeExpr::Str)
        .param_default("flags", TypeExpr::Int, e_int(0)).param_default("context", t_mixed(), e_null())
        .returns(result_type()).returning(e_method_call(e_var("finfo"), "__elephc_file", vec![e_str("finfo_file"), e_var("filename"), e_var("flags"), e_var("context")])).build());
    out.push(function("mime_content_type").param("filename", t_mixed()).returns(result_type()).body(vec![
        s_assign("_finfo", e_new_fq("finfo", vec![e_const("FILEINFO_MIME_TYPE")])),
        s_if(e_call("is_resource", vec![e_var("filename")]), super::stream_magic::resource_query(), vec![], None),
        mime_type_check(),
        reject(eq(e_var("filename"), e_str("")), "ValueError", "mime_content_type(): Argument #1 ($filename) must not be empty"),
        reject(e_binop(e_call("strpos", vec![e_var("filename"), e_str("\0")]), BinOp::StrictNotEq, e_bool(false)), "TypeError", "mime_content_type(): Argument #1 ($filename) must not contain any null bytes"),
        s_return(e_method_call(e_var("_finfo"), "__elephc_file", vec![e_str("mime_content_type"), e_var("filename")])),
    ]).build());
    if php_version >= PhpVersion::Php85 {
        for statement in &mut out {
            if let StmtKind::FunctionDecl { name, .. } = &statement.kind {
                if name.as_str() == "finfo_close" {
                    let args = [("since", "8.5"), ("message", "as finfo objects are freed automatically")]
                        .into_iter().map(|(name, value)| Expr::new(ExprKind::NamedArg {
                            name: name.to_string(), value: Box::new(e_str(value)),
                        }, Span::dummy())).collect();
                    statement.attributes.push(attr("\\Deprecated", args));
                }
            }
        }
    }
    out
}

/// Keeps each query's descriptor classification local while recursive readers borrow its output flag.
fn stream_query() -> Vec<Stmt> {
    super::pending::query()
}

/// Builds externs first, followed by the class and six PHP functions.
pub(super) fn declarations(php_version: PhpVersion) -> Program {
    internal_declarations(|| {
        let mut out = vec![
            extern_fn("elephc_fileinfo_open", "elephc_fileinfo").param("flags", CType::Int).param("path", CType::Str).returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_free", "elephc_fileinfo").param("handle", CType::Int).returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_set_flags", "elephc_fileinfo").param("handle", CType::Int).param("flags", CType::Int).returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_file", "elephc_fileinfo").param("handle", CType::Int).param("path", CType::Str).param("flags", CType::Int).returns(CType::Str).build(),
            extern_fn("elephc_fileinfo_buffer", "elephc_fileinfo").param("handle", CType::Int).param("data", CType::Str).param("length", CType::Int).param("flags", CType::Int).returns(CType::Str).build(),
            extern_fn("elephc_fileinfo_ok", "elephc_fileinfo").returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_error", "elephc_fileinfo").returns(CType::Str).build(),
            extern_fn("elephc_fileinfo_open_failed", "elephc_fileinfo").returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_get_flags", "elephc_fileinfo").param("handle", CType::Int).returns(CType::Int).build(),
            extern_fn("elephc_runtime_stream_has_wrapper", "System").param("scheme", CType::Str).returns(CType::Int).build(),
            extern_fn("elephc_runtime_stream_reset_error", "System").build(),
            extern_fn("elephc_runtime_stream_error_code", "System").returns(CType::Int).build(),
            extern_fn("elephc_runtime_stream_tls_error", "System").returns(CType::Str).build(),
            extern_fn("elephc_fileinfo_network_error", "elephc_fileinfo").param("code", CType::Int).param("detail", CType::Str).returns(CType::Str).build(),
            extern_fn("elephc_fileinfo_read", "elephc_fileinfo").param("path", CType::Str).returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_read_data", "elephc_fileinfo").param("id", CType::Int).returns(CType::Ptr).build(),
            extern_fn("elephc_fileinfo_read_length", "elephc_fileinfo").param("id", CType::Int).returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_read_release", "elephc_fileinfo").param("id", CType::Int).build(),
            extern_fn("elephc_runtime_stream_missing_stat_class", "System").param("id", CType::Int).returns(CType::Ptr).build(),
            extern_fn("elephc_runtime_stream_stat_name", "System").param("row", CType::Ptr).returns(CType::Ptr).build(),
            extern_fn("elephc_runtime_stream_stat_name_length", "System").param("row", CType::Ptr).returns(CType::Int).build(),
            extern_fn("elephc_runtime_stream_close_mode", "System").param("mode", CType::Int).returns(CType::Int).build(),
            extern_fn("elephc_runtime_stream_class", "System").param("id", CType::Int).returns(CType::Ptr).build(),
            extern_fn("elephc_runtime_fileinfo_cast", "System").param("id", CType::Int).returns(CType::Ptr).build(),
            extern_fn("elephc_runtime_fileinfo_cast_status", "System").param("result", CType::Ptr).returns(CType::Int).build(),
            extern_fn("elephc_runtime_fileinfo_cast_release", "System").param("result", CType::Ptr).build(),
            extern_fn("elephc_runtime_stream_exception_warning", "System").param("message", CType::Str).build(),
            handle_owner(),
            finfo_class(),
        ];
        out.extend(functions(php_version));
        out
    })
}

/// Preserves the public call name, argument number and profile's filename exception class.
fn filename_check(null_bytes: bool) -> Stmt {
    let condition = if null_bytes {
        e_binop(e_call("strpos", vec![e_var("filename"), e_str("\0")]), BinOp::StrictNotEq, e_bool(false))
    } else {
        eq(e_var("filename"), e_str(""))
    };
    let message = concat(concat(e_var("_function"), e_ternary(
        eq(e_var("_function"), e_str("finfo_file")),
        e_str("(): Argument #2 ($filename) "), e_str("(): Argument #1 ($filename) "),
    )), e_str(if null_bytes { "must not contain any null bytes" } else { "must not be empty" }));
    let error = s_throw(e_new_fq("ValueError", vec![message.clone()]));
    let error = if null_bytes {
        s_if(e_binop(e_const("PHP_VERSION_ID"), BinOp::Lt, e_int(80500)),
            vec![s_throw(e_new_fq("TypeError", vec![message]))], vec![], Some(vec![error]))
    } else { error };
    s_if(condition, vec![error], vec![], None)
}

/// Formats local stream-open errors separately from errors returned by the magic engine.
fn query_failure(file: bool) -> Vec<Stmt> {
    let prefix = if file {
        e_ternary(eq(e_call("elephc_fileinfo_open_failed", vec![]), e_int(1)),
            concat(concat(concat(e_var("_function"), e_str("(")), e_var("_source_filename")), e_str("): Failed to open stream: ")),
            concat(e_var("_function"), e_str("(): ")))
    } else { concat(e_var("_function"), e_str("(): ")) };
    vec![s_expr(e_call("__elephc_builtin_warning", vec![concat(prefix, e_call("elephc_fileinfo_error", vec![]))])), s_return(e_bool(false))]
}

/// Reports the PHP parameter number and ZPP type name for a non-resource context.
fn context_type_error(function: Expr) -> Vec<Stmt> {
    let type_name = php_type(e_var("context"));
    let argument = e_ternary(e_binop(eq(function.clone(), e_str("finfo_file")), BinOp::Or,
        eq(function.clone(), e_str("finfo_buffer"))), e_str("(): Argument #4 ($context) "), e_str("(): Argument #3 ($context) "));
    vec![s_throw(e_new_fq("TypeError", vec![concat(concat(concat(function, argument),
        e_str("must be of type resource or null, ")), concat(type_name, e_str(" given")))]))]
}

/// Returns the Zend argument-error spelling for a PHP value's runtime type.
fn php_type(value: Expr) -> Expr {
    let name = e_call("gettype", vec![value.clone()]);
    e_match(name.clone(), vec![
        (vec![e_str("integer")], e_str("int")), (vec![e_str("double")], e_str("float")),
        (vec![e_str("boolean")], e_str("bool")), (vec![e_str("NULL")], e_str("null")),
        (vec![e_str("object")], e_call("get_class", vec![value])),
    ], Some(name))
}

/// Includes the offending runtime type in mime_content_type's mixed-argument diagnostic.
fn mime_type_check() -> Stmt {
    s_if(e_not(e_call("is_string", vec![e_var("filename")])), vec![
        s_throw(e_new_fq("TypeError", vec![concat(e_str("mime_content_type(): Argument #1 ($filename) must be of type resource|string, "),
            concat(php_type(e_var("filename")), e_str(" given")))])),
    ], vec![], None)
}

/// Identifies values rejected by PHP's nullable-resource parameter parsing.
fn non_resource_context() -> Expr {
    e_binop(e_binop(e_var("context"), BinOp::StrictNotEq, e_null()), BinOp::And,
        e_not(e_call("is_resource", vec![e_var("context")])))
}

/// Rejects malformed buffer contexts before emitting the PHP 8.5 deprecation.
fn buffer_context(function: &str, supplied: i64) -> Stmt {
    s_if(non_resource_context(), context_type_error(e_str(function)), vec![], Some(vec![
        deprecated(&format!("{function}(): The $context parameter has no effect for finfo_buffer()"), Some(supplied)),
    ]))
}

/// Makes temporary options observable to stream/error callbacks and restores them on every exit.
fn with_query_flags(body: Vec<Stmt>) -> Vec<Stmt> {
    vec![
        s_assign("_restore_flags", e_binop(e_var("flags"), BinOp::StrictNotEq, e_int(0))),
        s_assign("_flags_handle", e_this_prop("__elephc_handle")),
        s_assign("_saved_flags", e_int(0)),
        s_if(e_var("_restore_flags"), vec![
            s_assign("_saved_flags", e_call("elephc_fileinfo_get_flags", vec![e_var("_flags_handle")])),
            s_expr(e_call("elephc_fileinfo_set_flags", vec![e_var("_flags_handle"), e_var("flags")])),
        ], vec![], None),
        s_try(body, vec![], Some(vec![s_if(e_var("_restore_flags"), vec![
            s_expr(e_call("elephc_fileinfo_set_flags", vec![e_var("_flags_handle"), e_var("_saved_flags")])),
        ], vec![], None)])),
    ]
}
