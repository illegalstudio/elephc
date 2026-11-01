//! Purpose:
//! Home of the PHP `array_replace` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `fixed(&["array", "replacements"])` (two required
//!   params, no variadic), matching the registry signature. The
//!   param-derived bounds already require exactly 2 arguments, so no `min_args`/
//!   `max_args` override is needed; `check_arity` owns the arity contract.
//! - `check` enforces that both arguments are associative arrays or
//!   indexed arrays of scalars, and the result is the two-input hash result type. A
//!   check hook is required because the return type depends on the inferred arguments.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_replace",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayReplace,
    ),
}

/// Validates both arguments are hash-compatible arrays and returns the merged hash type.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. Both arguments are
/// re-inferred here to drive the return type; the registry already inferred every
/// argument once for side effects. Each operand must be an associative array or an
/// indexed array of scalars; the result widens key/value to `Mixed` when the operands
/// disagree, via `PhpType::two_input_hash_result`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    // php's signature is `array_replace(array $array, array ...$replacements)`. Reading exactly two
    // arguments refused ordinary PHP with `takes exactly 2 arguments` -- Symfony's `UrlGenerator`
    // writes `array_replace($defaults, $this->context->getParameters(), $parameters)`, and that
    // one line kept the whole routing component out of the compiled world.
    let accepted = |t: &PhpType| {
        matches!(
            t,
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_)
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
    if types.iter().any(|ty| matches!(ty, PhpType::Mixed | PhpType::Union(_))) {
        return Ok(PhpType::Mixed);
    }
    // Left to right, exactly as the nested two-argument lowering evaluates it.
    let mut result = types.first().cloned().unwrap_or(PhpType::Mixed);
    for ty in types.iter().skip(1) {
        result = PhpType::two_input_hash_result(&result, ty);
    }
    Ok(result)
}
