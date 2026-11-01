//! Purpose:
//! Home of the PHP `strtr` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - php-src exposes one function with two shapes: `strtr($string, $from, $to)` translates
//!   bytes pairwise (truncated to the shorter of `$from`/`$to`), and `strtr($string, $pairs)`
//!   applies replacement pairs longest-match-first in a single left-to-right pass.
//! - `$from` is declared `Mixed` because it is `array|string` in php-src; the check hook
//!   enforces php-src's own `TypeError` wording at compile time, where elephc can already see
//!   the argument's type.
//! - The two-argument form applies PHP string coercion to boxed replacement values at runtime;
//!   statically-string hashes keep their direct pointer/length fast path.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "strtr",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Strtr,
    ),
}

/// Validates the `strtr` call shape and returns its `string` result type.
///
/// Argument types are inferred by the common registry dispatch path before this hook fires,
/// and arity is pre-validated by the registry. The two-argument form requires an array
/// `$from`, and the three-argument form requires a string `$from`; both shape mismatches carry
/// php-src's own `TypeError` wording.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let from = cx.checker.infer_type(from_argument(cx.args), cx.env)?;
    if cx.args.len() >= 3 {
        if matches!(from, PhpType::Array(_) | PhpType::AssocArray { .. }) {
            return Err(CompileError::new(
                cx.span,
                "strtr(): Argument #2 ($from) must be of type string, array given",
            ));
        }
        return Ok(PhpType::Str);
    }
    match &from {
        PhpType::Array(_) | PhpType::AssocArray { .. } => {}
        // A gradual `$from` is what an untyped helper hands back, and the backend dispatches on
        // the runtime tag for it. Twig's `CoreExtension::replace()` does
        // `strtr($str ?? '', self::toArray($from))`, where `toArray()` is untyped.
        ty if gradual_pairs_type(ty) => {}
        _ => {
            return Err(CompileError::new(
                cx.span,
                "strtr(): Argument #2 ($from) must be of type array, string given",
            ))
        }
    }
    Ok(PhpType::Str)
}

/// Returns whether `$from`'s type leaves the array/string choice to run time.
///
/// `Mixed` always does. A union qualifies only when one of its members IS an array, so a union of
/// scalars keeps php-src's `TypeError` wording at compile time rather than deferring a call that
/// can never be the two-argument form.
fn gradual_pairs_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Mixed => true,
        PhpType::Union(members) => members
            .iter()
            .any(|member| matches!(member, PhpType::Array(_) | PhpType::AssocArray { .. })),
        _ => false,
    }
}

/// Returns the `$from` argument expression from a call's source-order argument list.
///
/// A `from:` named argument is matched by name first so `strtr($s, from: [...])` validates
/// like the positional spelling; otherwise the second positional argument is used. The
/// registry guarantees at least two arguments before this hook runs, so the caller always
/// gets an expression back.
fn from_argument(args: &[crate::parser::ast::Expr]) -> &crate::parser::ast::Expr {
    for arg in args {
        if let ExprKind::NamedArg { name, value } = &arg.kind {
            if name == "from" {
                return value;
            }
        }
    }
    args.iter()
        .filter(|arg| !matches!(arg.kind, ExprKind::NamedArg { .. }))
        .nth(1)
        .unwrap_or(&args[args.len() - 1])
}
