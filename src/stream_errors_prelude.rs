//! Purpose:
//! Declares PHP 8.6's stream error enums, value objects, exception, and helper functions.
//!
//! Called from:
//! - `crate::core_prelude::inject_if_used` when a PHP 8.6 program reaches the stream error surface.
//!
//! Key details:
//! - `fopen()` and `stream_context_set_default()` are wrapped in ordinary PHP functions so their
//!   error handling runs through the normal checker/EIR path on every target.
//! - The last operation's errors live in `$GLOBALS['__elephc_stream_errors']`, which
//!   `stream_last_errors()` and `stream_clear_errors()` expose.
//! - The wrapper bodies are the only calls the rewrite pass leaves pointing at the original
//!   builtins.

use crate::parser::ast::{
    BinOp, EnumCaseDecl, Expr, ExprKind, Program, Stmt, StmtKind, TypeExpr,
};
use crate::span::Span;
use crate::synthetic_class::{
    class, e_array, e_binop, e_bool, e_call, e_class_const, e_const, e_index, e_int, e_instance_of,
    e_new, e_not, e_null, e_null_coalesce, e_parent_call, e_static_prop, e_str, e_this, e_this_prop, e_var,
    method, s_assign, s_expr, s_if, s_prop_assign, s_return, s_return_void, s_static_prop_assign,
    s_throw, t_array, t_class, t_mixed, t_nullable,
};

/// Builds the complete PHP 8.6 stream-error surface.
pub(crate) fn declarations() -> Program {
    let mut program = vec![
        enum_decl("StreamErrorCode", &STREAM_ERROR_CODES),
        enum_decl("StreamErrorMode", &["Error", "Exception", "Silent"]),
        enum_decl(
            "StreamErrorStore",
            &["Auto", "None", "NonTerminating", "Terminating", "All"],
        ),
        stream_error_class(),
        stream_exception_class(),
        stream_last_errors_function(),
        stream_clear_errors_function(),
        stream_error_store_class(),
        stream_failure_helper(),
        fopen_wrapper(),
        stream_context_set_default_wrapper(),
    ];
    program.shrink_to_fit();
    program
}

/// Returns whether `usage` reaches any stream error symbol or a wrapped stream call.
pub(crate) fn requested(usage: &crate::prelude_prune::usage::Usage) -> bool {
    usage.functions.iter().any(|name| {
        name == "fopen"
            || name == "stream_context_set_default"
            || name == "stream_last_errors"
            || name == "stream_clear_errors"
            || name.starts_with("stream_context_")
    }) || usage
        .classes
        .iter()
        .any(|name| name.starts_with("streamerror") || name == "streamexception")
}

/// The PHP-visible `StreamErrorCode` cases, in upstream declaration order.
const STREAM_ERROR_CODES: [&str; 75] = [
    "None",
    "Generic",
    "ReadFailed",
    "WriteFailed",
    "SeekFailed",
    "SeekNotSupported",
    "FlushFailed",
    "TruncateFailed",
    "ConnectFailed",
    "BindFailed",
    "ListenFailed",
    "AcceptFailed",
    "NotWritable",
    "NotReadable",
    "Disabled",
    "NotFound",
    "PermissionDenied",
    "AlreadyExists",
    "InvalidPath",
    "PathTooLong",
    "OpenFailed",
    "CreateFailed",
    "DupFailed",
    "UnlinkFailed",
    "RenameFailed",
    "MkdirFailed",
    "RmdirFailed",
    "StatFailed",
    "MetaFailed",
    "ChmodFailed",
    "ChownFailed",
    "CopyFailed",
    "TouchFailed",
    "InvalidMode",
    "InvalidMeta",
    "ModeNotSupported",
    "Readonly",
    "RecursionDetected",
    "NotImplemented",
    "NoOpener",
    "PersistentNotSupported",
    "WrapperNotFound",
    "WrapperDisabled",
    "ProtocolUnsupported",
    "WrapperRegistrationFailed",
    "WrapperUnregistrationFailed",
    "WrapperRestorationFailed",
    "FilterNotFound",
    "FilterFailed",
    "CastFailed",
    "CastNotSupported",
    "MakeSeekableFailed",
    "BufferedDataLost",
    "NetworkSendFailed",
    "NetworkRecvFailed",
    "SslNotSupported",
    "ResumptionFailed",
    "SocketPathTooLong",
    "OobNotSupported",
    "ProtocolError",
    "InvalidUrl",
    "InvalidResponse",
    "InvalidHeader",
    "InvalidParam",
    "RedirectLimit",
    "AuthFailed",
    "TimeOut",
    "ArchivingFailed",
    "EncodingFailed",
    "DecodingFailed",
    "InvalidFormat",
    "AllocationFailed",
    "TemporaryFileFailed",
    "LockFailed",
    "LockNotSupported",
];

/// Builds one unit enum with the supplied case names.
fn enum_decl(name: &str, cases: &[&str]) -> Stmt {
    Stmt::new(
        StmtKind::EnumDecl {
            name: name.to_string(),
            generics: None,
            backing_type: None,
            cases: cases
                .iter()
                .map(|case| EnumCaseDecl {
                    name: (*case).to_string(),
                    value: None,
                    span: Span::dummy(),
                    attributes: Vec::new(),
                })
                .collect(),
            implements: Vec::new(),
            trait_uses: Vec::new(),
            methods: Vec::new(),
            constants: Vec::new(),
        },
        Span::dummy(),
    )
}

/// Builds the `StreamError` readonly value object.
fn stream_error_class() -> Stmt {
    class("StreamError")
        .final_()
        .readonly_prop("code", t_class("StreamErrorCode"))
        .readonly_prop("message", TypeExpr::Str)
        .readonly_prop("wrapperName", TypeExpr::Str)
        .readonly_prop("severity", TypeExpr::Int)
        .readonly_prop("terminating", TypeExpr::Bool)
        .method(
            method("__construct")
                .param("code", t_class("StreamErrorCode"))
                .param("message", TypeExpr::Str)
                .param("wrapperName", TypeExpr::Str)
                .param("severity", TypeExpr::Int)
                .param("terminating", TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "code", e_var("code")),
                    s_prop_assign(e_this(), "message", e_var("message")),
                    s_prop_assign(e_this(), "wrapperName", e_var("wrapperName")),
                    s_prop_assign(e_this(), "severity", e_var("severity")),
                    s_prop_assign(e_this(), "terminating", e_var("terminating")),
                ]),
        )
        .build()
}

/// Builds `StreamException` with the engine's private error list.
fn stream_exception_class() -> Stmt {
    class("StreamException")
        .extends("Exception")
        .private_prop("errors", t_array(), Some(e_array(vec![])))
        .method(
            method("__construct")
                .param_default("message", TypeExpr::Str, e_str(""))
                .param_default("code", TypeExpr::Int, e_int(0))
                .param_default("previous", t_nullable(t_class("Throwable")), e_null())
                .param_default("errors", t_array(), e_array(vec![]))
                .body(vec![
                    s_expr(e_parent_call(
                        "__construct",
                        vec![e_var("message"), e_var("code"), e_var("previous")],
                    )),
                    s_prop_assign(e_this(), "errors", e_var("errors")),
                ]),
        )
        .method(method("getErrors").returns(t_array()).returning(e_this_prop("errors")))
        .build()
}

/// Builds the shared process-global stream error store.
fn stream_error_store_class() -> Stmt {
    class("__ElephcStreamErrors")
        .static_prop("errors", t_array(), Some(e_array(vec![])))
        .build()
}

/// Builds `stream_last_errors(): array`.
fn stream_last_errors_function() -> Stmt {
    crate::synthetic_class::function("stream_last_errors")
        .returns(t_array())
        .body(vec![s_return(e_static_prop(
            "__ElephcStreamErrors",
            "errors",
        ))])
        .build()
}

/// Builds `stream_clear_errors(): void`.
fn stream_clear_errors_function() -> Stmt {
    crate::synthetic_class::function("stream_clear_errors")
        .returns(TypeExpr::Void)
        .body(vec![
            s_static_prop_assign("__ElephcStreamErrors", "errors", e_array(vec![])),
            s_return_void(),
        ])
        .build()
}

/// Builds the shared failure reporter used by the `fopen()` wrapper.
fn stream_failure_helper() -> Stmt {
    let options = e_var("options");
    let stream = e_var("stream");
    let mode = e_var("mode");
    let store = e_var("store");
    let error = e_var("error");
    let handler = e_var("handler");
    crate::synthetic_class::function("__elephc_stream_failure")
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign(
                "options",
                e_call(
                    "stream_context_get_options",
                    vec![e_call("stream_context_get_default", vec![])],
                ),
            ),
            s_assign(
                "stream",
                e_null_coalesce(e_index(options.clone(), e_str("stream")), e_array(vec![])),
            ),
            s_assign(
                "mode",
                e_null_coalesce(
                    e_index(stream.clone(), e_str("error_mode")),
                    e_class_const("StreamErrorMode", "Error"),
                ),
            ),
            s_if(
                e_not(e_instance_of(mode.clone(), "StreamErrorMode")),
                vec![s_expr(e_call(
                    "trigger_error",
                    vec![
                        e_str("fopen(): Failed to open stream: operation failed"),
                        e_const("E_USER_WARNING"),
                    ],
                ))],
                Vec::new(),
                Some(vec![s_if(
                    e_binop(
                        mode.clone(),
                        BinOp::StrictEq,
                        e_class_const("StreamErrorMode", "Error"),
                    ),
                    vec![s_expr(e_call(
                        "trigger_error",
                        vec![
                            e_str("fopen(): Failed to open stream: operation failed"),
                            e_const("E_USER_WARNING"),
                        ],
                    ))],
                    Vec::new(),
                    None,
                )]),
            ),
            s_if(
                e_not(e_instance_of(mode.clone(), "StreamErrorMode")),
                vec![s_throw(e_new(
                    "TypeError",
                    vec![e_str(
                        "stream context option 'error_mode' must be of type StreamErrorMode",
                    )],
                ))],
                vec![],
                None,
            ),
            s_assign(
                "store",
                e_null_coalesce(
                    e_index(stream.clone(), e_str("error_store")),
                    e_class_const("StreamErrorStore", "Auto"),
                ),
            ),
            s_if(
                e_not(e_instance_of(store.clone(), "StreamErrorStore")),
                vec![s_throw(e_new(
                    "TypeError",
                    vec![e_str(
                        "stream context option 'error_store' must be of type StreamErrorStore",
                    )],
                ))],
                vec![],
                None,
            ),
            s_static_prop_assign("__ElephcStreamErrors", "errors", e_array(vec![])),
            s_assign(
                "error",
                e_new(
                    "StreamError",
                    vec![
                        e_class_const("StreamErrorCode", "OpenFailed"),
                        e_str("Failed to open stream: operation failed"),
                        e_str("PHP"),
                        e_const("E_WARNING"),
                        e_bool(true),
                    ],
                ),
            ),
            s_if(
                e_binop(
                    store.clone(),
                    BinOp::StrictEq,
                    e_class_const("StreamErrorStore", "Auto"),
                ),
                vec![s_if(
                    e_binop(
                        mode.clone(),
                        BinOp::StrictEq,
                        e_class_const("StreamErrorMode", "Silent"),
                    ),
                    vec![s_static_prop_assign(
                        "__ElephcStreamErrors",
                        "errors",
                        e_array(vec![error.clone()]),
                    )],
                    vec![],
                    None,
                )],
                Vec::new(),
                Some(vec![s_if(
                    e_binop(
                        store.clone(),
                        BinOp::StrictEq,
                        e_class_const("StreamErrorStore", "All"),
                    ),
                    vec![s_static_prop_assign(
                        "__ElephcStreamErrors",
                        "errors",
                        e_array(vec![error.clone()]),
                    )],
                    Vec::new(),
                    Some(vec![s_if(
                        e_binop(
                            store.clone(),
                            BinOp::StrictEq,
                            e_class_const("StreamErrorStore", "Terminating"),
                        ),
                        vec![s_static_prop_assign(
                            "__ElephcStreamErrors",
                            "errors",
                            e_array(vec![error.clone()]),
                        )],
                        Vec::new(),
                        None,
                    )]),
                )]),
            ),
            s_assign(
                "handler",
                e_null_coalesce(e_index(stream, e_str("error_handler")), e_null()),
            ),
            s_if(
                e_binop(handler.clone(), BinOp::StrictNotEq, e_null()),
                vec![s_expr(e_call(
                    "call_user_func",
                    vec![handler, e_array(vec![error.clone()])],
                ))],
                Vec::new(),
                None,
            ),
            s_if(
                e_binop(
                    mode,
                    BinOp::StrictEq,
                    e_class_const("StreamErrorMode", "Exception"),
                ),
                vec![s_throw(e_new(
                    "StreamException",
                    vec![
                        e_str("Failed to open stream: operation failed"),
                        e_int(21),
                        e_null(),
                        e_array(vec![error]),
                    ],
                ))],
                Vec::new(),
                None,
            ),
        ])
        .build()
}

/// Builds the `fopen()` wrapper that reports failures through the stream error surface.
fn fopen_wrapper() -> Stmt {
    crate::synthetic_class::function("__elephc_fopen")
        .param("filename", TypeExpr::Str)
        .param("mode", TypeExpr::Str)
        .param_default("use_include_path", TypeExpr::Bool, e_bool(false))
        .param_default("context", t_mixed(), e_null())
        .returns(t_mixed())
        .body(vec![
            s_assign("stream", e_null()),
            s_if(
                e_binop(e_var("context"), BinOp::StrictEq, e_null()),
                vec![s_assign(
                    "stream",
                    Expr::new(
                        ExprKind::ErrorSuppress(Box::new(e_call(
                            "fopen",
                            vec![
                                e_var("filename"),
                                e_var("mode"),
                                e_var("use_include_path"),
                            ],
                        ))),
                        Span::dummy(),
                    ),
                )],
                vec![],
                Some(vec![s_assign(
                    "stream",
                    Expr::new(
                        ExprKind::ErrorSuppress(Box::new(e_call(
                            "fopen",
                            vec![
                                e_var("filename"),
                                e_var("mode"),
                                e_var("use_include_path"),
                                e_var("context"),
                            ],
                        ))),
                        Span::dummy(),
                    ),
                )]),
            ),
            s_if(
                e_binop(e_var("stream"), BinOp::StrictEq, e_bool(false)),
                vec![s_expr(e_call("__elephc_stream_failure", vec![]))],
                Vec::new(),
                None,
            ),
            s_return(e_var("stream")),
        ])
        .build()
}

/// Builds the `stream_context_set_default()` wrapper that rejects stream error options.
fn stream_context_set_default_wrapper() -> Stmt {
    let options = e_var("options");
    let stream = e_index(options.clone(), e_str("stream"));
    let has_stream_options = e_call("is_array", vec![stream.clone()]);
    let has_error_option = or(
        e_call(
            "array_key_exists",
            vec![e_str("error_mode"), stream.clone()],
        ),
        or(
            e_call(
                "array_key_exists",
                vec![e_str("error_store"), stream.clone()],
            ),
            e_call(
                "array_key_exists",
                vec![e_str("error_handler"), stream],
            ),
        ),
    );
    crate::synthetic_class::function("__elephc_stream_context_set_default")
        .param("options", t_array())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(has_stream_options, BinOp::And, has_error_option),
                vec![s_throw(e_new(
                    "ValueError",
                    vec![e_str(
                        "Stream error handling options cannot be set on the default context",
                    )],
                ))],
                Vec::new(),
                None,
            ),
            s_return(e_call("stream_context_set_default", vec![options])),
        ])
        .build()
}


/// Returns `left || right`.
fn or(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::Or, right)
}
