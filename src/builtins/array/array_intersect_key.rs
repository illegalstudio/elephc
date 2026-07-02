//! Purpose:
//! Home of the PHP `array_intersect_key` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&["array"], "arrays")` (one regular `array`
//!   param plus a variadic `arrays`). The legacy CHECK arm required exactly 2 arguments,
//!   so `min_args: 2, max_args: 2` reproduce that enforcement in `check_arity` only;
//!   `function_sig` and the parity gate keep the variadic shape from the golden.
//! - `check` reproduces the legacy rule: the first argument must be an indexed or
//!   associative array, and the result preserves that first-operand type. A check hook
//!   is required because the return type depends on the inferred first-argument type.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_intersect_key` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_intersect_key",
    area: Array,
    params: [array: Mixed],
    variadic: "arrays",
    min_args: 2,
    max_args: 2,
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Computes the intersection of arrays using keys for comparison.",
    php_manual: "https://www.php.net/manual/en/function.array-intersect-key.php",
}

/// Validates the first argument is an array and returns its (preserved) type.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. The first argument is
/// re-inferred here to drive the return type; the registry already inferred every
/// argument once for side effects. The result preserves the first-operand array shape.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty1 = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty1, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be array", cx.name),
        ));
    }
    Ok(ty1)
}

/// Lowers an `array_intersect_key` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_array_intersect_key(ctx, inst)
}
