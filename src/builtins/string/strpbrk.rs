//! Purpose:
//! Home of PHP's `strpbrk` declaration and typed runtime target.
//!
//! Called from:
//! - The shared builtin registry through `crate::builtins::string`.
//!
//! Key details:
//! - The result is `string|false`, represented as a fresh boxed `Mixed` value.
//! - An empty character list is PHP's catchable `ValueError`, not a no-match result.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "strpbrk",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Strpbrk,
    ),
}

/// Returns PHP's `string|false` result contract for every `strpbrk()` invocation.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx
        .checker
        .normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
