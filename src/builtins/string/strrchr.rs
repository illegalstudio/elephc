//! Purpose:
//! Home of the PHP `strrchr` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The result type is `string|false`: PHP returns `false` when the needle's first byte does
//!   not occur. `Union(Str, False)` has backend representation `Mixed`, so the lowering boxes
//!   both arms, like `strstr()`.
//! - `$before_needle` (PHP 8.3) is accepted and materialized through the shared truthiness
//!   helper.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "strrchr",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Strrchr,
    ),
}

/// Returns `string|false` for every `strrchr()` call, matching the boxed lowering.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx
        .checker
        .normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
