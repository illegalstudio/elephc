//! Purpose:
//! Home of the PHP `array_key_exists` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` accepts concrete arrays plus boxed `Mixed`/union containers used after runtime
//!   `is_array()` guards, and returns `Bool`.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_key_exists` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_key_exists",
    area: Array,
    params: [key: Mixed, array: Mixed],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Checks if the given key or index exists in the array.",
    php_manual: "https://www.php.net/manual/en/function.array-key-exists.php",
}

/// Validates that the second argument can carry an array and returns `Bool`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// Boxed `Mixed` and union values are accepted because guarded arrays retain their dynamic packed
/// versus associative representation; lowering dispatches their runtime tags to the correct probe.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let arr_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(
        arr_ty,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Err(CompileError::new(
            cx.span,
            "array_key_exists() second argument must be array",
        ));
    }
    Ok(PhpType::Bool)
}

/// Lowers an `array_key_exists` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_key_exists(ctx, inst)
}
