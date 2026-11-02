//! Purpose:
//! Home of the PHP `abs` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required because the return type depends on the argument type:
//!   `Float` input returns `Float`, `Mixed`/Union-containing-Float returns `Mixed`,
//!   and all other inputs return `Int`.
//! - A runtime `Int` argument returns `Mixed`, not `Int`: `abs(PHP_INT_MIN)` has no `int`
//!   representation and PHP promotes it to `float(9.2233720368547758E+18)`. This mirrors how
//!   `$a + $b` on two runtime ints is already typed `Mixed` so the checked helper can promote
//!   on overflow. An `int` *literal* argument is still exact, so it keeps the precise `Int`
//!   (or `Mixed` for the single overflowing literal) result.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

builtin! {
    contract: "abs",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Abs,
    ),
}

/// Returns the most precise result type for `abs($num)` based on the argument type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(match ty {
        PhpType::Float => PhpType::Float,
        PhpType::Mixed => PhpType::Mixed,
        PhpType::Union(ref members) if members.iter().any(|m| *m == PhpType::Float) => {
            PhpType::Mixed
        }
        PhpType::Union(ref members) if members.iter().any(|m| *m == PhpType::Mixed) => {
            PhpType::Mixed
        }
        PhpType::Int => int_abs_result_type(&cx.args[0]),
        // A numeric string converts to an int OR a float (`crate::numeric_string_arg_prelude`).
        PhpType::Str => PhpType::Mixed,
        _ => PhpType::Int,
    })
}

/// Returns the result type of `abs()` applied to an `int`-typed argument expression.
///
/// A literal is exact: every value except `PHP_INT_MIN` has an `int` absolute value, and the
/// one that does not still needs the boxed `Mixed` result so the backend can hand back the
/// promoted float. A runtime `int` could be `PHP_INT_MIN`, so it must stay `Mixed`.
fn int_abs_result_type(arg: &Expr) -> PhpType {
    match &arg.kind {
        ExprKind::IntLiteral(value) if value.checked_abs().is_some() => PhpType::Int,
        _ => PhpType::Mixed,
    }
}
