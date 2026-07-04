//! Purpose:
//! Home of the PHP `array_key_last` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is an array (or Mixed) and returns `Mixed`.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_key_last` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_key_last",
    area: Array,
    params: [array: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Gets the last key of an array.",
    php_manual: "https://www.php.net/manual/en/function.array-key-last.php",
}

/// Validates that the argument is an array or Mixed and returns `Mixed`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
/// Mixed is permitted because heterogeneous arrays are represented as Mixed at compile time.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed) {
        return Err(CompileError::new(
            cx.span,
            "array_key_last() argument must be array",
        ));
    }
    Ok(PhpType::Mixed)
}

/// Lowers an `array_key_last` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_array_key_last(ctx, inst)
}
