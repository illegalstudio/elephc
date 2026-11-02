//! Purpose:
//! Builds the by-reference form of `exec()` -- `exec($command, &$output, &$result_code)` -- as a
//! prelude declaration.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program calls `exec()` with more than one argument.
//! - `crate::ir_lower::expr::compat_preludes`, which routes such a call to it.
//!
//! Key details:
//! - The native `exec()` answers the command's last line and nothing else. php also appends every
//!   output line to `$output` and stores the exit status in `$result_code`; both are ordinary
//!   by-reference parameters here, so no runtime symbol is needed.
//! - php-src's `php_exec` strips trailing whitespace (`isspace`: space, `\t`, `\n`, `\v`, `\f`,
//!   `\r`) from every line it hands back, appends to an `$output` that already holds elements,
//!   and answers the last line it read -- an empty string for a command that printed nothing.
//! - The status is the exit code decoded from `pclose()`'s raw wait status (`-1` for a signal death).
//! - Every `exec()` call takes this route, the one-argument form included: the native helper
//!   answered the whole output untrimmed, where php answers the last line.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_int, e_not, e_null, e_str, e_ternary, e_var, function,
    php_semantics_declarations, s_array_push, s_assign, s_expr, s_if, s_return, s_while,
};

/// Reserved helper the multi-argument `exec()` call is routed to.
pub(crate) const EXEC_FULL_NAME: &str = "__elephc_exec_full";

/// Reserved helper the two-argument `passthru()` call is routed to.
pub(crate) const PASSTHRU_FULL_NAME: &str = "__elephc_passthru_full";

/// Returns the helper declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![decl_fn_exec_full(), decl_fn_passthru_full()])
}

/// Builds `__elephc_passthru_full(string $command, &$result_code = null): mixed`.
///
/// php's `passthru()` hands the command's output straight to the output layer and stores the exit
/// status in `$result_code`; it answers `null`, or `false` when the process cannot be started.
/// Laravel's `DevCommand::runViaMultiplex()` returns that status as the command's exit code.
fn decl_fn_passthru_full() -> Stmt {
    function(PASSTHRU_FULL_NAME)
        .param("command", TypeExpr::Str)
        .param_by_ref_default("result_code", None, e_null())
        .body(vec![
            s_assign("handle", e_call("popen", vec![e_var("command"), e_str("r")])),
            s_if(
                e_binop(e_var("handle"), BinOp::StrictEq, e_bool(false)),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_expr(e_call("fpassthru", vec![e_var("handle")])),
            s_assign("status", e_call("pclose", vec![e_var("handle")])),
            s_assign(
                "result_code",
                e_ternary(
                    e_binop(
                        e_binop(e_var("status"), BinOp::BitAnd, e_int(127)),
                        BinOp::StrictEq,
                        e_int(0),
                    ),
                    e_binop(
                        e_binop(e_var("status"), BinOp::ShiftRight, e_int(8)),
                        BinOp::BitAnd,
                        e_int(255),
                    ),
                    e_int(-1),
                ),
            ),
            s_return(e_null()),
        ])
        .build()
}

/// Builds `__elephc_exec_full(string $command, &$output = null, &$result_code = null): string`.
fn decl_fn_exec_full() -> Stmt {
    function(EXEC_FULL_NAME)
        .param("command", TypeExpr::Str)
        .param_by_ref_default("output", None, e_null())
        .param_by_ref_default("result_code", None, e_null())
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("handle", e_call("popen", vec![e_var("command"), e_str("r")])),
            s_if(
                e_binop(e_var("handle"), BinOp::StrictEq, e_bool(false)),
                vec![s_return(e_str(""))],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("is_array", vec![e_var("output")])),
                vec![s_assign("output", e_array(vec![]))],
                vec![],
                None,
            ),
            s_assign("last", e_str("")),
            s_assign("line", e_call("fgets", vec![e_var("handle")])),
            s_while(
                e_binop(e_var("line"), BinOp::StrictNotEq, e_bool(false)),
                vec![
                    s_assign(
                        "last",
                        e_call(
                            "rtrim",
                            vec![e_var("line"), e_str(" \t\n\r\u{b}\u{c}")],
                        ),
                    ),
                    s_array_push("output", e_var("last")),
                    s_assign("line", e_call("fgets", vec![e_var("handle")])),
                ],
            ),
            // `pclose()` answers the raw wait status; php stores the exit code.
            s_assign("status", e_call("pclose", vec![e_var("handle")])),
            s_assign(
                "result_code",
                e_ternary(
                    e_binop(
                        e_binop(e_var("status"), BinOp::BitAnd, e_int(127)),
                        BinOp::StrictEq,
                        e_int(0),
                    ),
                    e_binop(
                        e_binop(e_var("status"), BinOp::ShiftRight, e_int(8)),
                        BinOp::BitAnd,
                        e_int(255),
                    ),
                    e_int(-1),
                ),
            ),
            s_return(e_var("last")),
        ])
        .build()
}
