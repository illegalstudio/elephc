//! Purpose:
//! Builds PHP's `proc_open()` family -- `proc_open`, `proc_close`, `proc_get_status`,
//! `proc_terminate`, `proc_nice` -- as prelude declarations over the `elephc_proc` bridge, for
//! programs that name any of them.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declarations when the
//!   program references a member of the family and declares none of the names itself.
//!
//! Key details:
//! - The bridge (`crates/elephc-proc`) starts the child and answers wait queries; everything PHP
//!   owns stays here: the `$descriptor_spec` walk, the `$pipes` array, the stream resources.
//!   `__elephc_fd_to_stream()` wraps a parent pipe end as an ordinary stream resource, so
//!   `fread`, `fwrite`, `fgets`, `stream_get_contents`, `stream_select`, `stream_set_blocking`
//!   and `fclose` work on it unchanged. `__elephc_stream_fd()` goes the other way for a stream
//!   resource named in the spec.
//! - The process handle is a stream resource over `/dev/null`: its descriptor number is the key
//!   the bridge's status table uses. `proc_close()` retires the table entry but does NOT `fclose()`
//!   the handle: the caller's box and any copy of it each own that descriptor number, so closing
//!   one copy would leave another to close the number again after it was reused (a reassigned
//!   `$p` then closed the next process's token). The box releases it at scope exit instead, so
//!   `is_resource()` stays true after `proc_close()` and `get_resource_type()` reads `stream`.
//! - `$pipes` is an untyped by-reference parameter, which is php's own declaration: a caller
//!   that passes an untyped property (`protected $_pipes = null;`) gets it retyped by the
//!   by-reference property rule in the checker.
//! - Pipes the caller left open are closed by the bridge itself at `proc_close()`: it recorded each
//!   parent end's pipe identity at registration and swaps a still-matching descriptor for
//!   `/dev/null`, so the child sees end-of-file (as php's process destructor guarantees) without
//!   the descriptor number being freed under the caller's stream resource.
//! - Spec validation mirrors php-src's messages: structural mistakes throw `ValueError`, while an
//!   unknown descriptor kind or an unknown redirect target warns and returns `false`.
//! - Built as AST, never parsed from PHP source. The interpreter has its own homes
//!   (`interpreter::builtins::filesystem::proc_open`) over the same bridge crate.

use crate::parser::ast::{BinOp, CType, CastType, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_array_assoc, e_binop, e_bool, e_call, e_cast, e_concat_all, e_const, e_index, e_int,
    e_neg, e_new, e_not, e_null, e_null_coalesce, e_str, e_ternary, e_var, extern_fn, function,
    php_semantics_declarations, s_array_assign, s_array_push, s_assign, s_expr, s_foreach, s_if,
    s_return, s_throw, t_array, t_nullable, t_union,
};

/// The bridge the externs bind to; declaring them is what links `libelephc_proc.a`.
const PROC_BRIDGE: &str = "elephc_proc";

/// The PHP-visible functions this prelude declares, for injection and polyfill checks.
pub(crate) const FUNCTIONS: &[&str] = &[
    "proc_open",
    "proc_close",
    "proc_get_status",
    "proc_terminate",
    "proc_nice",
];

/// Returns the declarations for the members of the family the program does not declare itself.
///
/// The extern block and the two internal helpers are always included: any member needs them.
pub(crate) fn declarations(wanted: &[&str]) -> Program {
    php_semantics_declarations(|| {
        let mut program = externs();
        program.push(decl_fn_token());
        for name in wanted {
            program.push(match *name {
                "proc_open" => decl_fn_proc_open(),
                "proc_close" => decl_fn_proc_close(),
                "proc_get_status" => decl_fn_proc_get_status(),
                "proc_terminate" => decl_fn_proc_terminate(),
                _ => decl_fn_proc_nice(),
            });
        }
        program
    })
}

/// One bridge function whose parameters are `int` or, where `shape` has an `s`, `string`.
fn bridge_fn(name: &str, shape: &str, returns: CType) -> Stmt {
    let mut builder = extern_fn(name, PROC_BRIDGE);
    for (index, kind) in shape.chars().enumerate() {
        let (label, c_type) = match kind {
            's' => (format!("s{index}"), CType::Str),
            _ => (format!("i{index}"), CType::Int),
        };
        builder = builder.param(&label, c_type);
    }
    builder.returns(returns).build()
}

/// The `elephc_proc` extern block.
fn externs() -> Program {
    vec![
        bridge_fn("elephc_proc_new", "i", CType::Int),
        bridge_fn("elephc_proc_free", "i", CType::Void),
        bridge_fn("elephc_proc_add_arg", "is", CType::Int),
        bridge_fn("elephc_proc_set_cwd", "is", CType::Int),
        bridge_fn("elephc_proc_begin_env", "i", CType::Int),
        bridge_fn("elephc_proc_add_env", "is", CType::Int),
        bridge_fn("elephc_proc_desc_pipe", "iii", CType::Int),
        bridge_fn("elephc_proc_desc_file", "iiss", CType::Int),
        bridge_fn("elephc_proc_desc_fd", "iii", CType::Int),
        bridge_fn("elephc_proc_desc_null", "ii", CType::Int),
        bridge_fn("elephc_proc_desc_redirect", "iii", CType::Int),
        bridge_fn("elephc_proc_spawn", "i", CType::Int),
        bridge_fn("elephc_proc_parent_fd", "ii", CType::Int),
        bridge_fn("elephc_proc_register", "ii", CType::Int),
        bridge_fn("elephc_proc_poll", "i", CType::Int),
        bridge_fn("elephc_proc_info", "ii", CType::Int),
        bridge_fn("elephc_proc_command", "i", CType::Str),
        bridge_fn("elephc_proc_close", "i", CType::Int),
        bridge_fn("elephc_proc_terminate", "ii", CType::Int),
        bridge_fn("elephc_proc_known", "i", CType::Int),
        bridge_fn("elephc_proc_nice", "i", CType::Int),
        bridge_fn("elephc_proc_strerror", "i", CType::Str),
    ]
}

/// A call into the bridge.
fn bridge(name: &str, args: Vec<Expr>) -> Expr {
    e_call(name, args)
}

/// `left === right`.
fn is_identical(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::StrictEq, right)
}

/// `(int) value`.
fn int_of(value: Expr) -> Expr {
    e_cast(CastType::Int, value)
}

/// `throw new ValueError(message);`
fn throw_value_error(message: &str) -> Stmt {
    s_throw(e_new("ValueError", vec![e_str(message)]))
}

/// Frees the builder, then throws a `ValueError` -- the shape of every spec validation failure.
fn fail_with_value_error(message: &str) -> Vec<Stmt> {
    vec![
        s_expr(bridge("elephc_proc_free", vec![e_var("handle")])),
        throw_value_error(message),
    ]
}

/// Frees the builder, warns, and returns `false` -- php's non-throwing spec failures.
fn fail_with_warning(message: Expr) -> Vec<Stmt> {
    vec![
        s_expr(bridge("elephc_proc_free", vec![e_var("handle")])),
        s_expr(e_call("trigger_error", vec![message, e_const("E_USER_WARNING")])),
        s_return(e_bool(false)),
    ]
}

/// Builds `__elephc_proc_token($process, string $function): int`: the validated table key.
fn decl_fn_token() -> Stmt {
    function("__elephc_proc_token")
        .param_untyped("process")
        .param("function", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_call("is_resource", vec![e_var("process")])),
                vec![s_throw(e_new(
                    "TypeError",
                    vec![e_concat_all(vec![
                        e_var("function"),
                        e_str("(): Argument #1 ($process) must be of type resource, "),
                        e_call("get_debug_type", vec![e_var("process")]),
                        e_str(" given"),
                    ])],
                ))],
                vec![],
                None,
            ),
            s_assign("token", e_call("__elephc_stream_fd", vec![e_var("process")])),
            s_if(
                e_binop(
                    bridge("elephc_proc_known", vec![e_var("token")]),
                    BinOp::StrictNotEq,
                    e_int(1),
                ),
                vec![s_throw(e_new(
                    "TypeError",
                    vec![e_concat_all(vec![
                        e_var("function"),
                        e_str("(): supplied resource is not a valid process resource"),
                    ])],
                ))],
                vec![],
                None,
            ),
            s_return(e_var("token")),
        ])
        .build()
}

/// `$spec[<index>] ?? null` compared with `=== null`: the "parameter is missing" test.
fn spec_missing(index: i64) -> Expr {
    is_identical(
        e_null_coalesce(e_index(e_var("spec"), e_int(index)), e_null()),
        e_null(),
    )
}

/// One `$descriptor_spec` entry that is an array: dispatches on its first element.
fn array_spec_arms() -> Stmt {
    let spec_at = |index: i64| e_index(e_var("spec"), e_int(index));
    s_if(
        is_identical(e_var("kind"), e_str("pipe")),
        vec![
            s_if(
                spec_missing(1),
                fail_with_value_error("Missing mode parameter for 'pipe'"),
                vec![],
                None,
            ),
            s_assign("mode", e_cast(CastType::String, spec_at(1))),
            s_expr(bridge(
                "elephc_proc_desc_pipe",
                vec![
                    e_var("handle"),
                    e_var("fd"),
                    e_ternary(
                        is_identical(
                            e_call("substr", vec![e_var("mode"), e_int(0), e_int(1)]),
                            e_str("r"),
                        ),
                        e_int(1),
                        e_int(0),
                    ),
                ],
            )),
            s_array_push("pipe_fds", e_var("fd")),
        ],
        vec![
            (
                is_identical(e_var("kind"), e_str("file")),
                vec![
                    s_if(
                        spec_missing(1),
                        fail_with_value_error("Missing file name parameter for 'file'"),
                        vec![],
                        None,
                    ),
                    s_if(
                        spec_missing(2),
                        fail_with_value_error("Missing mode parameter for 'file'"),
                        vec![],
                        None,
                    ),
                    s_expr(bridge(
                        "elephc_proc_desc_file",
                        vec![
                            e_var("handle"),
                            e_var("fd"),
                            e_cast(CastType::String, spec_at(1)),
                            e_cast(CastType::String, spec_at(2)),
                        ],
                    )),
                ],
            ),
            (
                is_identical(e_var("kind"), e_str("null")),
                vec![s_expr(bridge(
                    "elephc_proc_desc_null",
                    vec![e_var("handle"), e_var("fd")],
                ))],
            ),
            (
                is_identical(e_var("kind"), e_str("redirect")),
                vec![
                    s_if(
                        spec_missing(1),
                        fail_with_value_error("Missing redirection target"),
                        vec![],
                        None,
                    ),
                    s_assign("target", int_of(spec_at(1))),
                    s_if(
                        e_binop(
                            e_binop(e_var("target"), BinOp::Gt, e_int(2)),
                            BinOp::And,
                            e_not(e_call(
                                "array_key_exists",
                                vec![e_var("target"), e_var("descriptor_spec")],
                            )),
                        ),
                        fail_with_warning(e_concat_all(vec![
                            e_str("proc_open(): Redirection target "),
                            e_var("target"),
                            e_str(" not found"),
                        ])),
                        vec![],
                        None,
                    ),
                    s_expr(bridge(
                        "elephc_proc_desc_redirect",
                        vec![e_var("handle"), e_var("fd"), e_var("target")],
                    )),
                ],
            ),
        ],
        Some(fail_with_warning(e_concat_all(vec![
            e_str("proc_open(): "),
            e_var("kind"),
            e_str(" is not a valid descriptor spec/mode"),
        ]))),
    )
}

/// The `foreach ($descriptor_spec as $fd => $spec)` walk that declares every child descriptor.
fn descriptor_walk() -> Stmt {
    s_foreach(
        e_var("descriptor_spec"),
        Some("fd"),
        "spec",
        vec![
            s_if(
                e_not(e_call("is_int", vec![e_var("fd")])),
                fail_with_value_error(
                    "proc_open(): Argument #2 ($descriptor_spec) must be an integer indexed array",
                ),
                vec![],
                None,
            ),
            s_assign("fd", int_of(e_var("fd"))),
            s_if(
                e_call("is_resource", vec![e_var("spec")]),
                vec![s_expr(bridge(
                    "elephc_proc_desc_fd",
                    vec![
                        e_var("handle"),
                        e_var("fd"),
                        e_call("__elephc_stream_fd", vec![e_var("spec")]),
                    ],
                ))],
                vec![(
                    e_call("is_array", vec![e_var("spec")]),
                    vec![
                        s_if(
                            e_not(e_call("array_key_exists", vec![e_int(0), e_var("spec")])),
                            fail_with_value_error("Missing handle qualifier"),
                            vec![],
                            None,
                        ),
                        s_assign(
                            "kind",
                            e_cast(CastType::String, e_index(e_var("spec"), e_int(0))),
                        ),
                        array_spec_arms(),
                    ],
                )],
                Some(fail_with_value_error(
                    "proc_open(): Argument #2 ($descriptor_spec) must only contain arrays and streams",
                )),
            ),
        ],
    )
}

/// The command half of `proc_open()`: the shell line, or the program and its arguments.
fn command_arguments() -> Stmt {
    s_if(
        e_var("shell"),
        vec![s_expr(bridge(
            "elephc_proc_add_arg",
            vec![e_var("handle"), e_var("command")],
        ))],
        vec![],
        Some(vec![
            s_if(
                is_identical(e_call("count", vec![e_var("command")]), e_int(0)),
                fail_with_value_error("proc_open(): Argument #1 ($command) must not be empty"),
                vec![],
                None,
            ),
            s_assign("position", e_int(0)),
            s_foreach(
                e_var("command"),
                None,
                "argument",
                vec![
                    s_assign("position", e_binop(e_var("position"), BinOp::Add, e_int(1))),
                    s_assign("text", e_cast(CastType::String, e_var("argument"))),
                    s_if(
                        e_binop(
                            e_call("strpos", vec![e_var("text"), e_str("\0")]),
                            BinOp::StrictNotEq,
                            e_bool(false),
                        ),
                        fail_with_value_error_dynamic(),
                        vec![],
                        None,
                    ),
                    s_expr(bridge(
                        "elephc_proc_add_arg",
                        vec![e_var("handle"), e_var("text")],
                    )),
                ],
            ),
        ]),
    )
}

/// Frees the builder and throws php's "Command array element N contains a null byte".
fn fail_with_value_error_dynamic() -> Vec<Stmt> {
    vec![
        s_expr(bridge("elephc_proc_free", vec![e_var("handle")])),
        s_throw(e_new(
            "ValueError",
            vec![e_concat_all(vec![
                e_str("Command array element "),
                e_var("position"),
                e_str(" contains a null byte"),
            ])],
        )),
    ]
}

/// Builds `proc_open()`.
fn decl_fn_proc_open() -> Stmt {
    function("proc_open")
        .param("command", t_union(vec![t_array(), TypeExpr::Str]))
        .param("descriptor_spec", t_array())
        .param_by_ref("pipes", None)
        .param_default("cwd", t_nullable(TypeExpr::Str), e_null())
        .param_default("env_vars", t_nullable(t_array()), e_null())
        .param_default("options", t_nullable(t_array()), e_null())
        .body(vec![
            s_assign("shell", e_call("is_string", vec![e_var("command")])),
            s_assign(
                "handle",
                bridge(
                    "elephc_proc_new",
                    vec![e_ternary(e_var("shell"), e_int(1), e_int(0))],
                ),
            ),
            command_arguments(),
            s_assign("pipe_fds", e_array(vec![])),
            descriptor_walk(),
            s_if(
                e_binop(e_var("cwd"), BinOp::StrictNotEq, e_null()),
                vec![s_expr(bridge(
                    "elephc_proc_set_cwd",
                    vec![e_var("handle"), e_var("cwd")],
                ))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("env_vars"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(bridge("elephc_proc_begin_env", vec![e_var("handle")])),
                    s_foreach(
                        e_var("env_vars"),
                        Some("name"),
                        "value",
                        vec![s_if(
                            e_call("is_string", vec![e_var("name")]),
                            vec![s_expr(bridge(
                                "elephc_proc_add_env",
                                vec![
                                    e_var("handle"),
                                    e_concat_all(vec![
                                        e_var("name"),
                                        e_str("="),
                                        e_cast(CastType::String, e_var("value")),
                                    ]),
                                ],
                            ))],
                            vec![],
                            None,
                        )],
                    ),
                ],
                vec![],
                None,
            ),
            s_assign("pid", bridge("elephc_proc_spawn", vec![e_var("handle")])),
            s_if(
                e_binop(e_var("pid"), BinOp::Lt, e_int(0)),
                fail_with_warning(e_concat_all(vec![
                    e_str("proc_open(): posix_spawn() failed: "),
                    bridge("elephc_proc_strerror", vec![e_neg(e_var("pid"))]),
                ])),
                vec![],
                None,
            ),
            s_assign("streams", e_array(vec![])),
            s_foreach(
                e_var("pipe_fds"),
                None,
                "pipe_fd",
                vec![
                    s_assign(
                        "stream",
                        e_call(
                            "__elephc_fd_to_stream",
                            vec![bridge(
                                "elephc_proc_parent_fd",
                                vec![e_var("handle"), e_var("pipe_fd")],
                            )],
                        ),
                    ),
                    s_array_assign("streams", e_var("pipe_fd"), e_var("stream")),
                ],
            ),
            s_assign(
                "process",
                e_call("fopen", vec![e_str("/dev/null"), e_str("r")]),
            ),
            s_assign("token", e_call("__elephc_stream_fd", vec![e_var("process")])),
            s_expr(bridge(
                "elephc_proc_register",
                vec![e_var("handle"), e_var("token")],
            )),
            s_assign("pipes", e_var("streams")),
            s_return(e_var("process")),
        ])
        .build()
}

/// Builds `proc_close($process): int`.
fn decl_fn_proc_close() -> Stmt {
    function("proc_close")
        .param_untyped("process")
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign(
                "token",
                e_call(
                    "__elephc_proc_token",
                    vec![e_var("process"), e_str("proc_close")],
                ),
            ),
            s_assign("code", bridge("elephc_proc_close", vec![e_var("token")])),
            s_return(e_var("code")),
        ])
        .build()
}

/// `elephc_proc_info($token, <field>) === 1` for one boolean status field.
fn info_flag(field: i64) -> Expr {
    is_identical(info_int(field), e_int(1))
}

/// `elephc_proc_info($token, <field>)` for one integer status field.
fn info_int(field: i64) -> Expr {
    bridge("elephc_proc_info", vec![e_var("token"), e_int(field)])
}

/// Builds `proc_get_status($process): array`, in php 8.5's key order.
fn decl_fn_proc_get_status() -> Stmt {
    function("proc_get_status")
        .param_untyped("process")
        .returns(t_array())
        .body(vec![
            s_assign(
                "token",
                e_call(
                    "__elephc_proc_token",
                    vec![e_var("process"), e_str("proc_get_status")],
                ),
            ),
            s_expr(bridge("elephc_proc_poll", vec![e_var("token")])),
            // Bound to a local first: an extern string result written straight into an array
            // literal is never released (measured, one block per call).
            s_assign("command", bridge("elephc_proc_command", vec![e_var("token")])),
            s_return(e_array_assoc(vec![
                (e_str("command"), e_var("command")),
                (e_str("pid"), info_int(0)),
                (e_str("cached"), info_flag(7)),
                (e_str("running"), info_flag(1)),
                (e_str("signaled"), info_flag(2)),
                (e_str("stopped"), info_flag(3)),
                (e_str("exitcode"), info_int(4)),
                (e_str("termsig"), info_int(5)),
                (e_str("stopsig"), info_int(6)),
            ])),
        ])
        .build()
}

/// Builds `proc_terminate($process, int $signal = 15): bool`.
fn decl_fn_proc_terminate() -> Stmt {
    function("proc_terminate")
        .param_untyped("process")
        .param_default("signal", TypeExpr::Int, e_int(15))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign(
                "token",
                e_call(
                    "__elephc_proc_token",
                    vec![e_var("process"), e_str("proc_terminate")],
                ),
            ),
            s_return(is_identical(
                bridge("elephc_proc_terminate", vec![e_var("token"), e_var("signal")]),
                e_int(1),
            )),
        ])
        .build()
}

/// Builds `proc_nice(int $priority): bool`.
fn decl_fn_proc_nice() -> Stmt {
    function("proc_nice")
        .param("priority", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("result", bridge("elephc_proc_nice", vec![e_var("priority")])),
            s_if(
                is_identical(e_var("result"), e_int(-1000)),
                vec![
                    s_expr(e_call(
                        "trigger_error",
                        vec![
                            e_str("proc_nice(): Only a super user may attempt to increase the priority of a process"),
                            e_const("E_USER_WARNING"),
                        ],
                    )),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}
