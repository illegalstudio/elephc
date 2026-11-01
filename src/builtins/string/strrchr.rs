//! Purpose:
//! Home of PHP's `strrchr` declaration and typed runtime target.
//!
//! Called from:
//! - The shared builtin registry through `crate::builtins::string`.
//!
//! Key details:
//! - PHP searches for the first byte of the needle and returns the final matching suffix.
//! - A miss returns `false`, so the result uses the boxed `string|false` representation.

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

/// Returns PHP's `string|false` result contract for every `strrchr()` invocation.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx
        .checker
        .normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
