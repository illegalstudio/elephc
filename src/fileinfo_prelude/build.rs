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
    body.extend(replay_warnings(concat(e_var("_function"), e_str("(): "))));
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
        .param_untyped_default("context", e_null()).returns(result_type())
.body(body)
}

/// Builds the owning class, including PHP's clone and serialization restrictions.
fn finfo_class(open_basedir: Option<&str>) -> Stmt {
    let mut constructor = vec![s_if(
        e_binop(e_call("func_num_args", vec![]), BinOp::Gt, e_int(2)),
        vec![s_throw(e_new_fq("ArgumentCountError", vec![concat(
            e_str("finfo::__construct() expects at most 2 arguments, "),
            concat(e_call("func_num_args", vec![]), e_str(" given")))])),],
        vec![], None,
    )];
    constructor.extend(coerce_flags("finfo::__construct", 1, "flags"));
    constructor.extend(coerce_magic_database("finfo::__construct", "magic_database"));
    constructor.push(reject(e_binop(e_call("strpos", vec![e_cast(CastType::String, e_var("magic_database")), e_str("\0")]), BinOp::StrictNotEq, e_bool(false)), "ValueError", "finfo::__construct(): Argument #2 ($magic_database) must not contain any null bytes"));
    constructor.push(path_length_guard(true, open_basedir.is_some()));
    if let Some(roots) = open_basedir {
        constructor.push(open_basedir_guard(true, roots));
    }
    constructor.extend(missing_database("finfo_open", true));
    constructor.extend([
        s_assign("_database", e_cast(CastType::String, e_var("magic_database"))),
        s_prop_assign(e_this(), "__elephc_handle", e_int(0)),
        s_prop_assign(e_this(), "__elephc_owner", e_null()),
        s_assign("_raw", e_call("elephc_fileinfo_open", vec![e_var("flags"), e_var("_database")])),
    ]);
    constructor.push(s_if(eq(e_var("_raw"), e_int(0)), vec![
        s_if(e_binop(e_call("elephc_fileinfo_warnings_pending", vec![]), BinOp::Gt, e_int(0)),
            vec![s_throw(e_new_fq("Exception", vec![concat(e_str("finfo::__construct(): "), e_call("elephc_fileinfo_warning_first", vec![]))]))],
            vec![], Some(vec![s_throw(e_new_fq("Exception", vec![e_str("Constructor failed")]))])),
    ], vec![], None));
    // A successful load can still warn (for example MAGIC_CHECK's regular-file notice).
    // PHP's object constructor converts the first warning to an exception; the procedural
    // wrapper opts out and replays them after construction.
    constructor.push(s_if(
        e_binop(
            e_binop(e_not(e_static_prop("__ElephcFinfoHandle", "allow_warnings")), BinOp::And,
                e_binop(e_call("elephc_fileinfo_warnings_pending", vec![]), BinOp::Gt, e_int(0))),
            BinOp::And,
            e_binop(e_var("_raw"), BinOp::StrictNotEq, e_int(0))),
        vec![
            s_assign("_warning", e_call("elephc_fileinfo_warning_first", vec![])),
            s_expr(e_call("elephc_fileinfo_clear_warnings", vec![])),
            s_expr(e_call("elephc_fileinfo_free", vec![e_var("_raw")])),
            s_throw(e_new_fq("Exception", vec![concat(e_str("finfo::__construct(): "), e_var("_warning"))])),
        ],
        vec![], None,
    ));
    constructor.push(s_prop_assign(e_this(), "__elephc_owner", e_new_fq("__ElephcFinfoHandle", vec![e_var("_raw")])));
    constructor.push(s_prop_assign(e_this(), "__elephc_handle", e_var("_raw")));
    let mut set_flags = require_handle();
    set_flags.extend([s_expr(e_call("elephc_fileinfo_set_flags", vec![e_var("_raw"), e_var("flags")])), s_return(e_bool(true))]);
    class("finfo")
        .private_prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .private_prop("__elephc_owner", t_nullable(t_class("__ElephcFinfoHandle")), Some(e_null()))
        .method(method("__construct").param_default("flags", TypeExpr::Int, e_int(0))
            .param_default("magic_database", t_nullable(TypeExpr::Str), e_null())
            // The extra arguments exist only to reproduce PHP's internal-function
            // ArgumentCountError. A generated name keeps the slot out of reflection and
            // `get_defined_vars()` while `func_num_args()` still counts the call.
            .variadic(&crate::names::generated_local_name("__elephc_arguments"), Some(t_mixed())).body(constructor))
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
        // Procedural `finfo_open()` replays load warnings and returns the object; a direct
        // `new finfo()` follows PHP's EH_THROW and surfaces the first warning as an exception.
        .static_prop("allow_warnings", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("handle", TypeExpr::Int, Some(e_int(0)))
        .method(method("__construct").param("handle", TypeExpr::Int).body(vec![s_prop_assign(e_this(), "handle", e_var("handle"))]))
        .method(method("__destruct").body(vec![
            s_assign("_handle", e_this_prop("handle")),
            s_prop_assign(e_this(), "handle", e_int(0)),
            s_expr(e_call("elephc_fileinfo_free", vec![e_var("_handle")])),
        ])).build()
}

/// Builds procedural wrappers with the shared contract's PHP parameter names.
fn functions(php_version: PhpVersion, open_basedir: Option<&str>) -> Program {
    let mut open_body = coerce_flags("finfo_open", 1, "flags");
    open_body.extend(coerce_magic_database("finfo_open", "magic_database"));
    open_body.push(reject(e_binop(e_call("strpos", vec![e_cast(CastType::String, e_var("magic_database")), e_str("\0")]), BinOp::StrictNotEq, e_bool(false)), "ValueError", "finfo_open(): Argument #2 ($magic_database) must not contain any null bytes"));
    open_body.push(path_length_guard(false, open_basedir.is_some()));
    if let Some(roots) = open_basedir {
        open_body.push(open_basedir_guard(false, roots));
    }
    open_body.extend(missing_database("finfo_open", false));
    open_body.push(s_expr(e_assign(
        e_static_prop("__ElephcFinfoHandle", "allow_warnings"), e_bool(true))));
    let try_body = vec![
        s_assign("_opened", e_new_fq("finfo", vec![e_var("flags"), e_var("magic_database")])),
        s_expr(e_assign(
            e_static_prop("__ElephcFinfoHandle", "allow_warnings"), e_bool(false))),
    ];
    let mut catch_body = vec![s_expr(e_assign(
        e_static_prop("__ElephcFinfoHandle", "allow_warnings"), e_bool(false)))];
    catch_body.extend(replay_warnings(e_str("finfo_open(): ")));
    catch_body.push(warning(concat(e_str("finfo_open(): "), e_call("elephc_fileinfo_error", vec![]))));
    catch_body.push(s_return(e_bool(false)));
    open_body.push(s_try(
        try_body,
        vec![(vec!["Exception"], Some("_error"), catch_body)],
        Some(vec![s_expr(e_assign(
            e_static_prop("__ElephcFinfoHandle", "allow_warnings"), e_bool(false)))]),
    ));
    // Replay outside the try: an error handler that throws during a successful open must
    // propagate instead of being converted to `false`.
    open_body.extend(replay_warnings(e_str("finfo_open(): ")));
    open_body.push(s_return(e_var("_opened")));
    let mut out = vec![
        function("finfo_open").param_default("flags", TypeExpr::Int, e_int(0))
            .param_default("magic_database", t_nullable(TypeExpr::Str), e_null())
            .returns(t_union(vec![t_class("finfo"), TypeExpr::False]))
            .body(open_body).build(),
        function("finfo_close").param("finfo", t_class("finfo")).returns(TypeExpr::Bool).body({
            let mut body = vec![
                deprecated("Function finfo_close() is deprecated since 8.5, as finfo objects are freed automatically", None),
            ];
            body.extend(finfo_argument("finfo_close"));
            body.push(s_return(e_bool(true)));
            body
        }).build(),
        function("finfo_set_flags").param("finfo", t_class("finfo")).param("flags", TypeExpr::Int)
            .returns(TypeExpr::Bool).body({
                let mut body = finfo_argument("finfo_set_flags");
                body.push(s_return(e_method_call(e_var("finfo"), "__elephc_set_flags", vec![e_var("flags")])));
                body
            }).build(),
    ];
    out.push(function("finfo_buffer").param("finfo", t_class("finfo")).param("string", TypeExpr::Str)
        .param_default("flags", TypeExpr::Int, e_int(0)).param_untyped_default("context", e_null())
        .returns(result_type()).body({
            let mut body = finfo_argument("finfo_buffer");
            body.push(buffer_context("finfo_buffer", 3));
            body.push(s_return(e_method_call(e_var("finfo"), "__elephc_buffer", vec![e_str("finfo_buffer"), e_var("string"), e_var("flags"), e_var("context")])));
            body
        }).build());
    out.push(function("finfo_file").param("finfo", t_class("finfo")).param("filename", TypeExpr::Str)
        .param_default("flags", TypeExpr::Int, e_int(0)).param_untyped_default("context", e_null())
        .returns(result_type()).body({
            let mut body = finfo_argument("finfo_file");
            body.push(s_return(e_method_call(e_var("finfo"), "__elephc_file", vec![e_str("finfo_file"), e_var("filename"), e_var("flags"), e_var("context")])));
            body
        }).build());
    out.push(function("mime_content_type").param_untyped("filename").returns(result_type()).body(vec![
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
    let open_basedir = open_basedir_roots();
    internal_declarations(|| {
        let mut out = vec![
            extern_fn("elephc_fileinfo_open", "elephc_fileinfo").param("flags", CType::Int).param("path", CType::Str).returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_resolve_database", "elephc_fileinfo").param("path", CType::Str).returns(CType::Str).build(),
            extern_fn("elephc_fileinfo_path_allowed", "elephc_fileinfo").param("path", CType::Str).param("roots", CType::Str).returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_free", "elephc_fileinfo").param("handle", CType::Int).returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_set_flags", "elephc_fileinfo").param("handle", CType::Int).param("flags", CType::Int).returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_file", "elephc_fileinfo").param("handle", CType::Int).param("path", CType::Str).param("flags", CType::Int).returns(CType::Str).build(),
            extern_fn("elephc_fileinfo_buffer", "elephc_fileinfo").param("handle", CType::Int).param("data", CType::Str).param("length", CType::Int).param("flags", CType::Int).returns(CType::Str).build(),
            extern_fn("elephc_fileinfo_ok", "elephc_fileinfo").returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_error", "elephc_fileinfo").returns(CType::Str).build(),
            extern_fn("elephc_fileinfo_open_failed", "elephc_fileinfo").returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_warnings_pending", "elephc_fileinfo").returns(CType::Int).build(),
            extern_fn("elephc_fileinfo_warning_first", "elephc_fileinfo").returns(CType::Str).build(),
            extern_fn("elephc_fileinfo_warning_take", "elephc_fileinfo").returns(CType::Str).build(),
            extern_fn("elephc_fileinfo_clear_warnings", "elephc_fileinfo").build(),
            extern_fn("elephc_fileinfo_warnings_suppressed", "elephc_fileinfo").returns(CType::Int).build(),
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
            finfo_class(open_basedir.as_deref()),
        ];
        out.extend(functions(php_version, open_basedir.as_deref()));
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
    vec![warning(concat(prefix, e_call("elephc_fileinfo_error", vec![]))), s_return(e_bool(false))]
}

/// Returns the compile-time `open_basedir` spelling that confines magic database paths.
fn open_basedir_roots() -> Option<String> {
    crate::codegen_support::open_basedir_roots()
}

/// Rejects a magic database outside the compiled `open_basedir` roots.
///
/// PHP's `php_check_open_basedir` warning becomes a constructor `Exception` under the object's
/// throw-on-warning handler, while the procedural form reports it and returns false.
fn open_basedir_guard(constructor: bool, roots: &str) -> Stmt {
    let message = concat(
        e_str("open_basedir restriction in effect. File("),
        concat(e_var("magic_database"), concat(
            e_str(") is not within the allowed path(s): ("),
            concat(e_str(roots), e_str(")")))),
    );
    let outside = eq(
        e_call("elephc_fileinfo_path_allowed", vec![
            e_var("magic_database"), e_str(roots)]),
        e_int(0),
    );
    let condition = e_binop(
        e_binop(e_var("magic_database"), BinOp::StrictNotEq, e_null()), BinOp::And,
        e_binop(e_binop(e_var("magic_database"), BinOp::StrictNotEq, e_str("")), BinOp::And, outside),
    );
    let body = if constructor {
        vec![s_throw(e_new_fq("Exception", vec![concat(e_str("finfo::__construct(): "), message)]))]
    } else {
        vec![warning(concat(e_str("finfo_open(): "), message)), s_return(e_bool(false))]
    };
    s_if(condition, body, vec![], None)
}

/// Emits one PHP warning, escaping its text when the binary renders diagnostics as HTML.
pub(super) fn warning(message: Expr) -> Stmt {
    let message = if crate::codegen_support::html_errors_enabled() {
        // `htmlspecialchars` with ENT_SUBSTITUTE replaces invalid UTF-8 with U+FFFD.
        // elephc's byte-preserving helper does not validate, so do that first.
        let checked = e_ternary(
            e_binop(e_call("preg_match", vec![e_str("//u"), message.clone()]), BinOp::StrictEq, e_int(1)),
            message.clone(),
            e_call("preg_replace", vec![e_str("/[\\x80-\\xff]/"), e_str("\u{FFFD}"), message]),
        );
        e_call("htmlspecialchars", vec![checked])
    } else {
        message
    };
    s_expr(e_call("__elephc_builtin_warning", vec![message]))
}

/// Rejects a magic database name longer than PHP's platform path limit.
///
/// Without `open_basedir`, PHP's `expand_filepath` fails silently. With it, the
/// open_basedir check publishes the platform limit before giving up.
fn path_length_guard(constructor: bool, open_basedir: bool) -> Stmt {
    let message = concat(
        e_str("File name is longer than the maximum allowed path length on this platform ("),
        concat(e_const("PHP_MAXPATHLEN"), e_str("): ")),
    );
    let condition = e_binop(e_binop(e_var("magic_database"), BinOp::StrictNotEq, e_null()), BinOp::And,
        e_binop(e_call("strlen", vec![e_var("magic_database")]), BinOp::Gt, e_const("PHP_MAXPATHLEN")));
    let body = if constructor {
        if open_basedir {
            vec![s_throw(e_new_fq("Exception", vec![concat(e_str("finfo::__construct(): "),
                concat(message, e_var("magic_database")))]))]
        } else {
            vec![s_throw(e_new_fq("Exception", vec![e_str("Constructor failed")]))]
        }
    } else {
        let mut body = Vec::new();
        if open_basedir {
            body.push(warning(concat(e_str("finfo_open(): "), concat(message, e_var("magic_database")))));
        }
        body.push(s_return(e_bool(false)));
        body
    };
    s_if(condition, body, vec![], None)
}

/// Expands a magic database name exactly like PHP's CWD_EXPAND, including dot components.
fn resolved_database(variable: &str) -> Expr {
    e_call("elephc_fileinfo_resolve_database", vec![e_var(variable)])
}

/// Reports the stream-open warnings PHP emits before a missing magic database fails.
fn missing_database(function: &str, constructor: bool) -> Vec<Stmt> {
    let resolved = resolved_database("magic_database");
    let reason = "): Failed to open stream: No such file or directory";
    let mut body = Vec::new();
    if constructor {
        body.push(s_throw(e_new_fq("Exception", vec![concat(
            concat(e_str("finfo::__construct("), resolved),
            e_str(reason))])));
    } else {
        let warning_message = concat(
            concat(e_str(&format!("{function}(")), resolved.clone()),
            e_str(reason));
        body.push(warning(warning_message.clone()));
        body.push(warning(warning_message));
        body.push(warning(concat(e_str(&format!("{function}(): Failed to load magic database at \"")), concat(resolved, e_str("\"")))));
        body.push(s_return(e_bool(false)));
    }
    vec![s_if(e_binop(e_binop(e_var("magic_database"), BinOp::StrictNotEq, e_null()), BinOp::And,
        e_binop(e_binop(e_var("magic_database"), BinOp::StrictNotEq, e_str("")), BinOp::And,
            e_not(e_call("file_exists", vec![e_var("magic_database")])))), body, vec![], None)]
}

/// Emits every queued libmagic diagnostic with PHP's `function(): ` framing.
fn replay_warnings(prefix: Expr) -> Vec<Stmt> {
    let replay = s_while(
        e_binop(e_call("elephc_fileinfo_warnings_pending", vec![]), BinOp::Gt, e_int(0)),
        vec![warning(concat(prefix.clone(), e_call("elephc_fileinfo_warning_take", vec![])))],
    );
    // A hostile magic database can exceed the bridge's bounded queue; surface the
    // truncation once instead of silently hiding the remaining diagnostics.
    vec![replay, s_if(
        eq(e_call("elephc_fileinfo_warnings_suppressed", vec![]), e_int(1)),
        vec![warning(concat(prefix, e_str("additional fileinfo warnings suppressed")))],
        vec![], None,
    )]
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

/// Returns PHP's exact boolean spelling (`true`/`false`) and delegates every other type.
fn zend_type_name(value: Expr) -> Expr {
    e_ternary(e_call("is_bool", vec![value.clone()]),
        e_ternary(value.clone(), e_str("true"), e_str("false")), php_type(value))
}

/// Validates a mixed `$finfo` argument at one procedural function boundary.
fn finfo_argument(function: &str) -> Vec<Stmt> {
    vec![s_if(e_not(e_instance_of(e_var("finfo"), "finfo")), vec![
        s_throw(e_new_fq("TypeError", vec![concat(
            e_str(&format!("{function}(): Argument #1 ($finfo) must be of type finfo, ")),
            concat(zend_type_name(e_var("finfo")), e_str(" given")))])),
    ], vec![], None)]
}

/// Coerces a mixed `$flags` argument like ZPP's weak-mode `l` parser or throws its TypeError.
fn coerce_flags(function: &str, argument: usize, variable: &str) -> Vec<Stmt> {
    let message = format!("{function}(): Argument #{argument} ($flags) must be of type int, ");
    vec![s_if(e_not(e_call("is_int", vec![e_var(variable)])), vec![
        s_if(e_binop(e_call("is_numeric", vec![e_var(variable)]), BinOp::Or,
            e_call("is_bool", vec![e_var(variable)])),
            vec![s_assign(variable, e_cast(CastType::Int, e_var(variable)))],
            vec![], Some(vec![s_throw(e_new_fq("TypeError", vec![concat(
                e_str(&message), concat(zend_type_name(e_var(variable)), e_str(" given")))])),
            ])),
    ], vec![], None)]
}

/// Coerces scalar magic-database names like ZPP's `p!` parser or throws its TypeError.
fn coerce_magic_database(function: &str, variable: &str) -> Vec<Stmt> {
    let message = format!("{function}(): Argument #2 ($magic_database) must be of type ?string, ");
    vec![s_if(e_binop(e_binop(e_var(variable), BinOp::StrictNotEq, e_null()), BinOp::And,
        e_not(e_call("is_string", vec![e_var(variable)]))), vec![
        s_if(e_binop(e_call("is_int", vec![e_var(variable)]), BinOp::Or,
            e_binop(e_call("is_float", vec![e_var(variable)]), BinOp::Or,
                e_call("is_bool", vec![e_var(variable)]))),
            vec![s_assign(variable, e_cast(CastType::String, e_var(variable)))],
            vec![], Some(vec![s_throw(e_new_fq("TypeError", vec![concat(
                e_str(&message), concat(zend_type_name(e_var(variable)), e_str(" given")))])),
            ])),
    ], vec![], None)]
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
