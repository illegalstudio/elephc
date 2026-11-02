//! Purpose:
//! Home of the PHP `array_replace_recursive` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `fixed(&["array", "replacements"])` (two required
//!   params, no variadic), matching the registry signature. The
//!   param-derived bounds already require exactly 2 arguments, so no `min_args`/
//!   `max_args` override is needed; `check_arity` owns the arity contract.
//! - `check` accepts every PHP array representation. Gradual inputs use a Mixed result and a
//!   runtime-validated boundary; statically known arrays preserve a precise merged hash type.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_replace_recursive",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayReplaceRecursive,
    ),
}

/// Validates both arguments are hash-compatible arrays and returns the merged hash type.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. Both arguments are re-inferred here
/// to drive the return type; the registry already inferred every argument once for side effects.
/// Statically known arrays keep their precise merged hash type. Gradual inputs return `Mixed` so
/// lowering can validate their runtime array shapes.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    // php's signature is `array_replace_recursive(array $array, array ...$replacements)`; reading
    // exactly two refused ordinary PHP. See `array_replace` for the measurement that found it.
    let accepted = |t: &PhpType| {
        matches!(
            t,
            PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Mixed
                | PhpType::Union(_)
        )
    };
    let mut types = Vec::with_capacity(cx.args.len());
    for arg in cx.args {
        let ty = cx.checker.infer_type(arg, cx.env)?;
        if !accepted(&ty) {
            return Err(CompileError::new(
                cx.span,
                &format!("{}() arguments must be arrays", cx.name),
            ));
        }
        types.push(ty);
    }
    let requires_gradual = |t: &PhpType| matches!(t, PhpType::Mixed | PhpType::Union(_));
    if types.iter().any(requires_gradual) {
        return Ok(PhpType::Mixed);
    }
    let mut result = types.first().cloned().unwrap_or(PhpType::Mixed);
    for ty in types.iter().skip(1) {
        result = PhpType::two_input_hash_result(&result, ty);
    }
    Ok(result)
}
