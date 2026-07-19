//! Purpose:
//! Home of the PHP `array_replace_recursive` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `fixed(&["array", "replacements"])` (two required
//!   params, no variadic), matching the legacy `legacy_builtin_call_sig` arm. The
//!   param-derived bounds already require exactly 2 arguments, so no `min_args`/
//!   `max_args` override is needed; `check_arity` reproduces the legacy CHECK arity.
//! - `check` reproduces the legacy rule: both arguments must be associative arrays or
//!   indexed arrays of scalars, and the result is the two-input hash result type. A
//!   check hook is required because the return type depends on the inferred arguments.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_replace_recursive` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_replace_recursive",
    area: Array,
    params: [array: Mixed, replacements: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Replaces elements from passed arrays into the first array recursively.",
    php_manual: "https://www.php.net/manual/en/function.array-replace-recursive.php",
}

/// Validates both arguments are hash-compatible arrays and returns the merged hash type.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. Both arguments are
/// re-inferred here to drive the return type; the registry already inferred every
/// argument once for side effects. Each operand must be an associative array or an
/// indexed array of scalars; the result widens key/value to `Mixed` when the operands
/// disagree, via `PhpType::two_input_hash_result`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty1 = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let ty2 = cx.checker.infer_type(&cx.args[1], cx.env)?;
    let accepted =
        |t: &PhpType| matches!(t, PhpType::AssocArray { .. }) || t.is_scalar_indexed_array();
    if !accepted(&ty1) || !accepted(&ty2) {
        return Err(CompileError::new(
            cx.span,
            &format!(
                "{}() arguments must be associative arrays or indexed arrays of scalars",
                cx.name
            ),
        ));
    }
    Ok(PhpType::two_input_hash_result(&ty1, &ty2))
}

/// Lowers an `array_replace_recursive` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_replace_recursive(ctx, inst)
}
