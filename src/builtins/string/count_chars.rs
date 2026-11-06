//! Purpose:
//! Home of the PHP `count_chars` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - php-src's return type is `array|string`, chosen by the `$mode` VALUE: modes `0`, `1`,
//!   and `2` build a byte-value keyed tally, while modes `3` and `4` render the used or
//!   unused bytes as a string. elephc compiles ahead of time, so `$mode` has to be an
//!   integer literal (constant folding runs before the checker, so a named constant still
//!   qualifies).
//! - A mode outside `0..=4` keeps the tally shape here and the backend raises php-src's
//!   catchable `ValueError` before the helper runs, so the runtime function is declared
//!   `MAY_THROW` rather than pure.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "count_chars",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CountChars,
    ),
}

/// Returns the `$mode`-dependent result type for a `count_chars` call.
///
/// Argument types are inferred by the common registry dispatch path before this hook fires,
/// and arity is pre-validated by the registry. Modes `3` and `4` return `string`; every other
/// literal (including the `0` default and the out-of-range values php-src rejects with a
/// `ValueError`) returns the `array<int,int>` tally.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let tally = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: Box::new(PhpType::Int),
    };
    let Some(mode) = mode_argument(cx.args) else {
        return Ok(tally);
    };
    let ExprKind::IntLiteral(mode) = mode.kind else {
        return Err(CompileError::aot_literal_required(
            cx.span,
            "count_chars() mode argument must be an integer literal in AOT mode",
        ));
    };
    match mode {
        3 | 4 => Ok(PhpType::Str),
        _ => Ok(tally),
    }
}

/// Returns the `$mode` argument expression from a call's source-order argument list.
///
/// A `mode:` named argument is matched by name first so `count_chars($s, mode: 3)` resolves
/// to the same result type as the positional spelling; otherwise the second positional
/// argument is used, skipping any named argument that occupies that slot.
fn mode_argument(args: &[crate::parser::ast::Expr]) -> Option<&crate::parser::ast::Expr> {
    for arg in args {
        if let ExprKind::NamedArg { name, value } = &arg.kind {
            if name == "mode" {
                return Some(value);
            }
        }
    }
    args.iter()
        .filter(|arg| !matches!(arg.kind, ExprKind::NamedArg { .. }))
        .nth(1)
}
