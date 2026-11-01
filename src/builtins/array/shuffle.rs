//! Purpose:
//! Home of the PHP `shuffle` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array"]))`: exactly 1 argument,
//!   the `array` param is by-reference. The `ref` marker is mandatory — it is what makes
//!   by-reference mutation lower correctly (ir_lower reads `ref_params` from the registry sig).
//! - `check` requires the argument be an Array or AssocArray, returning Bool.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "shuffle",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Shuffle,
    ),
}

/// Validates the argument type for a `shuffle` call.
///
/// Requires the argument be an indexed or associative array. Arity (exactly 1) is
/// pre-validated by the registry. Returns `Ok(PhpType::Bool)` on success.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    // The same predicate `sort()` and the rest of the by-reference family use. `shuffle()` alone
    // demanded a concrete shape, which refused `shuffle(self::toArray($item, false))` in twig's
    // `CoreExtension::shuffle()` -- an untyped helper's result is gradual by construction.
    if !crate::types::checker::builtins::arrays::array_arg_is_gradually_acceptable(&ty) {
        return Err(CompileError::new(cx.span, &format!("{}() argument must be array", cx.name)));
    }
    Ok(PhpType::Bool)
}
