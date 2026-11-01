//! Purpose:
//! Injects the elephc-PHP functions that render a Throwable the way php does: the
//! `getTraceAsString()` listing and the full `__toString()` text with its `Next` chain.
//!
//! Called from:
//! - `crate::pipeline::compile()` (and the codegen test harness) before type checking.
//! - The bodies of `Exception::__toString()`/`getTraceAsString()` and `Error`'s, declared in
//!   `crate::types::checker::builtin_types::exception`; every dispatch path reaches those bodies.
//!
//! Key details:
//! - The compact Throwable payload has no method bodies, so these used to be codegen intrinsics:
//!   `getTraceAsString()` answered `""` and `__toString()` answered the bare message, and an
//!   `echo $e` could even fail to link. php renders `#N FILE(LINE): Class->method(args)` lines
//!   ending in `#N {main}`, and `Class: message in FILE:LINE\nStack trace:\n...`, printing the
//!   previous exception first and joining with `\n\nNext `.
//! - Arguments follow php's `zend.exception_ignore_args=Off` spelling; a frame without `args` (the
//!   traces elephc records today) prints `()`. A string longer than 15 bytes is cut with `...`.
//! - The parameter is typed `Throwable` on purpose: an interface-typed receiver reaches the
//!   Throwable getter intrinsics, which a `mixed` receiver would not.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_call, e_cast, e_concat, e_concat_all, e_index, e_int, e_method_call, e_null,
    e_str, e_ternary, e_var, function, internal_declarations, s_assign, s_foreach, s_return,
    t_class,
};

/// Renders `getTraceAsString()` for a builtin Throwable.
pub(crate) const TRACE_STRING_FN: &str = "__elephc_throwable_trace_string";
/// Renders `__toString()` for a builtin Throwable, including its previous chain.
pub(crate) const TO_STRING_FN: &str = "__elephc_throwable_to_string";
/// Renders one trace argument.
const TRACE_ARG_FN: &str = "__elephc_throwable_trace_arg";

/// Inventory group holding the three helpers.
pub const GROUP: &str = "throwable_string";

/// Every function this prelude declares, which the declaration pruner keeps as roots: the calls
/// to them are created during lowering, after the pruner has looked for callers.
pub const FUNCTIONS: [&str; 3] = [TRACE_STRING_FN, TO_STRING_FN, TRACE_ARG_FN];

/// Prepends the functions.
///
/// Unconditionally: a Throwable reaches a string conversion from places no AST scan can bound
/// (an engine-raised `TypeError` caught and echoed, a logger formatting `{$e}`), and the three
/// bodies are small. The pruner keeps them as roots, see [`FUNCTIONS`].
pub fn inject(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let mut combined = declarations();
    // Recorded so reachability can see that these helpers only iterate arrays (a trace and its
    // argument list): without it their `foreach` loops rooted the iterator protocol by name on
    // every live class. They are exported roots, so being in the inventory cannot prune them.
    inventory.record_program(GROUP, &combined);
    inventory.record_array_iteration_only(GROUP);
    combined.extend(program);
    combined
}

/// `if (condition) { then } [else { otherwise }]`, without elseif arms.
fn if_(condition: crate::parser::ast::Expr, then_body: Vec<Stmt>, else_body: Option<Vec<Stmt>>) -> Stmt {
    crate::synthetic_class::s_if(condition, then_body, Vec::new(), else_body)
}

/// Builds the three functions.
fn declarations() -> Program {
    internal_declarations(|| vec![trace_arg_fn(), trace_string_fn(), to_string_fn()])
}

/// `function __elephc_throwable_trace_arg($arg): string`.
fn trace_arg_fn() -> Stmt {
    let is = |name: &str| e_call(name, vec![e_var("arg")]);
    function(TRACE_ARG_FN)
        .param_untyped("arg")
        .returns(TypeExpr::Str)
        .body(vec![
            if_(e_binop(e_var("arg"), BinOp::StrictEq, e_null()), vec![s_return(e_str("NULL"))], None),
            if_(
                is("is_bool"),
                vec![s_return(e_ternary(e_var("arg"), e_str("true"), e_str("false")))],
                None,
            ),
            if_(
                is("is_string"),
                vec![
                    if_(
                        e_binop(e_call("strlen", vec![e_var("arg")]), BinOp::Gt, e_int(15)),
                        vec![s_return(e_concat_all(vec![
                            e_str("'"),
                            e_call("substr", vec![e_var("arg"), e_int(0), e_int(15)]),
                            e_str("...'"),
                        ]))],
                        None,
                    ),
                    s_return(e_concat_all(vec![e_str("'"), e_var("arg"), e_str("'")])),
                ],
                None,
            ),
            if_(is("is_array"), vec![s_return(e_str("Array"))], None),
            if_(
                is("is_object"),
                vec![s_return(e_concat_all(vec![
                    e_str("Object("),
                    e_call("get_class", vec![e_var("arg")]),
                    e_str(")"),
                ]))],
                None,
            ),
            if_(
                is("is_resource"),
                vec![s_return(e_concat(
                    e_str("Resource id #"),
                    e_cast(CastType::String, e_cast(CastType::Int, e_var("arg"))),
                ))],
                None,
            ),
            s_return(e_cast(CastType::String, e_var("arg"))),
        ])
        .build()
}

/// `function __elephc_throwable_trace_string(Throwable $e): string`.
fn trace_string_fn() -> Stmt {
    let frame = |key: &str| e_index(e_var("frame"), e_str(key));
    let has = |key: &str| e_call("array_key_exists", vec![e_str(key), e_var("frame")]);
    let append = |value| s_assign("out", e_concat(e_var("out"), value));
    function(TRACE_STRING_FN)
        .param("e", t_class("Throwable"))
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("out", e_str("")),
            s_assign("i", e_int(0)),
            s_foreach(
                e_method_call(e_var("e"), "getTrace", vec![]),
                None,
                "frame",
                vec![
                    append(e_concat_all(vec![e_str("#"), e_var("i"), e_str(" ")])),
                    if_(
                        has("file"),
                        vec![append(e_concat_all(vec![
                            frame("file"),
                            e_str("("),
                            frame("line"),
                            e_str("): "),
                        ]))],
                        Some(vec![append(e_str("[internal function]: "))]),
                    ),
                    if_(
                        has("class"),
                        vec![append(e_concat(frame("class"), frame("type")))],
                        None,
                    ),
                    append(e_concat(frame("function"), e_str("("))),
                    if_(
                        has("args"),
                        vec![
                            s_assign("sep", e_str("")),
                            s_foreach(
                                frame("args"),
                                None,
                                "arg",
                                vec![
                                    append(e_concat(
                                        e_var("sep"),
                                        e_call(TRACE_ARG_FN, vec![e_var("arg")]),
                                    )),
                                    s_assign("sep", e_str(", ")),
                                ],
                            ),
                        ],
                        None,
                    ),
                    append(e_str(")\n")),
                    s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                ],
            ),
            s_return(e_concat_all(vec![e_var("out"), e_str("#"), e_var("i"), e_str(" {main}")])),
        ])
        .build()
}

/// `function __elephc_throwable_to_string(Throwable $e): string`.
fn to_string_fn() -> Stmt {
    function(TO_STRING_FN)
        .param("e", t_class("Throwable"))
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("prefix", e_str("")),
            s_assign("previous", e_method_call(e_var("e"), "getPrevious", vec![])),
            if_(
                e_binop(e_var("previous"), BinOp::StrictNotEq, e_null()),
                vec![s_assign(
                    "prefix",
                    e_concat(e_call(TO_STRING_FN, vec![e_var("previous")]), e_str("\n\nNext ")),
                )],
                None,
            ),
            s_assign("message", e_method_call(e_var("e"), "getMessage", vec![])),
            s_return(e_concat_all(vec![
                e_var("prefix"),
                e_call("get_class", vec![e_var("e")]),
                e_ternary(
                    e_binop(e_var("message"), BinOp::StrictEq, e_str("")),
                    e_str(""),
                    e_concat(e_str(": "), e_var("message")),
                ),
                e_str(" in "),
                e_method_call(e_var("e"), "getFile", vec![]),
                e_str(":"),
                e_method_call(e_var("e"), "getLine", vec![]),
                e_str("\nStack trace:\n"),
                e_call(TRACE_STRING_FN, vec![e_var("e")]),
            ])),
        ])
        .build()
}
