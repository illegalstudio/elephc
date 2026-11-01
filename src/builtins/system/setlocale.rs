//! Purpose:
//! Home of PHP's `setlocale` builtin and Elephc's deterministic C-locale compatibility model.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Locale arguments follow PHP's public call contract; the AOT runtime keeps its stable
//!   locale model and reports a string-or-false-compatible boxed result.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "setlocale",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Setlocale,
    ),
}

/// Infers all locale candidates and returns PHP's string-or-false result union.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}
