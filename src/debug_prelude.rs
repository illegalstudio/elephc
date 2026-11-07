//! Purpose:
//! Declares the elephc-native `elephc_break` helper plus the debug-only
//! `elephc_dump`, `elephc_trace_start` and `elephc_trace_end` helpers in PHP and
//! routes them to the internal capture event builtin.
//!
//! Called from:
//! - `crate::pipeline::compile()` before name resolution, in every build.
//!
//! Key details:
//! - `elephc_break()` exists in every build: a no-op stub outside `--debug` and a
//!   real stop inside an interactive/DAP session.
//! - The dump/trace helpers are debug-only and `--strict-php` skips the prelude so
//!   every helper stays extension-only.
//! - The single internal builtin carries a boxed mixed value plus nullable label
//!   and destination strings; `kind` selects dump, segment-start, segment-end or
//!   break.
//! - Injection is pay-for-use: unused functions are removed by the normal
//!   reachability pruning.

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_call, e_int, e_new_fq, e_not, e_null, e_str, e_var, function,
    internal_declarations, s_expr, s_if, s_throw, t_mixed,
};

/// Inventory group used to prune unused debug-prelude functions.
const INVENTORY_GROUP: &str = "debug-prelude";

/// Injects the debug helpers when the build carries the debug runtime.
pub(crate) fn inject_if_used(
    program: Program,
    debug_build: bool,
    strict_php: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if strict_php {
        return program;
    }
    let mut combined = if debug_build {
        internal_declarations(debug_declarations)
    } else {
        internal_declarations(non_debug_declarations)
    };
    inventory.record_program(INVENTORY_GROUP, &combined);
    combined.extend(program);
    combined
}

/// Builds the four debug-build helper declarations for injection and parity tests.
pub(crate) fn debug_declarations() -> Vec<Stmt> {
    vec![
        function("elephc_break")
            .returns(TypeExpr::Void)
            .body(vec![s_expr(e_call(
                "__elephc_debug_event",
                vec![e_int(3), e_null(), e_null(), e_str("auto")],
            ))])
            .build(),
        function("elephc_dump")
            .param("value", t_mixed())
            .param_default("label", TypeExpr::Nullable(Box::new(TypeExpr::Str)), e_null())
            .param_default("to", TypeExpr::Str, e_str("auto"))
            .returns(TypeExpr::Void)
            .body(vec![
                // PHP's ValueError is part of the helper contract; validate before the
                // native recorder sees the string, so an invalid destination never
                // records or writes a partial snapshot.
                s_if(
                    invalid_destination(),
                    vec![s_throw(e_new_fq(
                        "ValueError",
                        vec![e_str(
                            "elephc_dump(): Argument #3 ($to) must be 'auto', 'stderr', 'terminal' or 'html'",
                        )],
                    ))],
                    Vec::new(),
                    None,
                ),
                s_expr(e_call(
                    "__elephc_debug_event",
                    vec![e_int(0), e_var("value"), e_var("label"), e_var("to")],
                )),
            ])
            .build(),
        function("elephc_trace_start")
            .param("name", TypeExpr::Str)
            .returns(TypeExpr::Void)
            .body(vec![
                empty_segment_guard("elephc_trace_start"),
                s_expr(e_call(
                    "__elephc_debug_event",
                    vec![e_int(1), e_null(), e_var("name"), e_str("auto")],
                )),
            ])
            .build(),
        function("elephc_trace_end")
            .param("name", TypeExpr::Str)
            .returns(TypeExpr::Void)
            .body(vec![
                empty_segment_guard("elephc_trace_end"),
                s_expr(e_call(
                    "__elephc_debug_event",
                    vec![e_int(2), e_null(), e_var("name"), e_str("auto")],
                )),
            ])
            .build(),
    ]
}

/// Guard that raises PHP's `ValueError` for an empty trace-segment name.
fn empty_segment_guard(function: &str) -> Stmt {
    s_if(
        e_binop(e_var("name"), BinOp::StrictEq, e_str("")),
        vec![s_throw(e_new_fq(
            "ValueError",
            vec![e_str(&format!(
                "{function}(): Argument #1 ($name) must not be empty"
            ))],
        ))],
        Vec::new(),
        None,
    )
}

/// Builds `!($to === 'auto' || $to === 'stderr' || $to === 'terminal' || $to === 'html')`.
fn invalid_destination() -> crate::parser::ast::Expr {
    let mut valid: Option<crate::parser::ast::Expr> = None;
    for destination in ["auto", "stderr", "terminal", "html"] {
        let comparison = e_binop(e_var("to"), BinOp::StrictEq, e_str(destination));
        valid = Some(match valid {
            Some(previous) => e_binop(previous, BinOp::Or, comparison),
            None => comparison,
        });
    }
    e_not(valid.expect("at least one valid destination"))
}

/// Builds the non-debug `elephc_break()` stub.
///
/// The dump/trace helpers need the debug runtime and stay debug-only; a breakpoint
/// request is safe to accept and ignore in a normal build.
fn non_debug_declarations() -> Vec<Stmt> {
    vec![function("elephc_break")
        .returns(TypeExpr::Void)
        .body(Vec::new())
        .build()]
}
