//! Purpose:
//! Home of the PHP `str_word_count` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - php-src's return type is `array|int`, chosen by the `$format` VALUE. elephc is an
//!   ahead-of-time compiler, so the result storage must be known at compile time: the
//!   `$format` argument therefore has to be an integer literal (constant folding runs
//!   before the checker, so `str_word_count($s, MY_FORMAT)` still qualifies).
//! - Format `0` yields `int`, format `1` a list `array<string>`, and format `2` the
//!   byte-offset map `array<int, string>`. Any other literal keeps the `int` shape and the
//!   backend raises php-src's catchable `ValueError` before the helper runs, so the runtime
//!   function is declared `MAY_THROW` rather than pure.
//! - `$characters` is nullable in php-src; an omitted or empty character list produces the
//!   same word mask, so the backend passes a zero-length list for both.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "str_word_count",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StrWordCount,
    ),
}

/// Returns the `$format`-dependent result type for a `str_word_count` call.
///
/// Argument types are inferred by the common registry dispatch path before this hook fires,
/// and arity is pre-validated by the registry. The hook only reads the `$format` argument's
/// literal value: format `1` returns `array<string>`, format `2` returns `array<int,string>`,
/// and every other literal (including the `0` default and the out-of-range values php-src
/// rejects with a `ValueError`) returns `int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let Some(format) = format_argument(cx.args) else {
        return Ok(PhpType::Int);
    };
    let ExprKind::IntLiteral(format) = format.kind else {
        return Err(CompileError::aot_literal_required(
            cx.span,
            "str_word_count() format argument must be an integer literal in AOT mode",
        ));
    };
    match format {
        1 => Ok(PhpType::Array(Box::new(PhpType::Str))),
        2 => Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Str),
        }),
        _ => Ok(PhpType::Int),
    }
}

/// Returns the `$format` argument expression from a call's source-order argument list.
///
/// A `format:` named argument is matched by name first so `str_word_count($s, format: 1)`
/// resolves to the same result type as the positional spelling; otherwise the second
/// positional argument is used, skipping any named argument that occupies that slot.
fn format_argument(args: &[crate::parser::ast::Expr]) -> Option<&crate::parser::ast::Expr> {
    for arg in args {
        if let ExprKind::NamedArg { name, value } = &arg.kind {
            if name == "format" {
                return Some(value);
            }
        }
    }
    let positional = args
        .iter()
        .filter(|arg| !matches!(arg.kind, ExprKind::NamedArg { .. }))
        .nth(1)?;
    Some(positional)
}
