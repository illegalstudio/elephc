//! Purpose:
//! Home of the PHP `fsockopen` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that `error_code` (arg[2]) and `error_message` (arg[3]), if provided,
//!   Returns `Union(stream_resource, Bool)`. The two error outputs are declared `ref(Int)` /
//!   `ref(Str)`, which is what requires a variable there and gives it its type.
//! - Arguments are pre-inferred by the registry before the hook runs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "fsockopen",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fsockopen,
    ),
}

/// Returns PHP's `resource|false` result. The by-reference outputs need no check here: their
/// `ref(T)` declarations carry the rule.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::stream_resource(), PhpType::False]))
}
