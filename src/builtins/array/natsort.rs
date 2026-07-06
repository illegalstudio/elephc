//! Purpose:
//! Home of the PHP `natsort` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array"]))`: exactly 1 argument,
//!   the `array` param is by-reference. The `ref` marker is mandatory — it is what makes
//!   by-reference mutation lower correctly (ir_lower reads `ref_params` from the registry sig).
//! - `check` requires the argument be an Array or AssocArray, returning Void.
//! - `lower` is a thin wrapper over the shared `arrays::lower_natsort` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "natsort",
    area: Array,
    params: [ref array: Mixed],
    returns: Void,
    check: check,
    lower: lower,
    summary: "Sorts an array using a natural order algorithm.",
    php_manual: "https://www.php.net/manual/en/function.natsort.php",
}

/// Validates the argument type for a `natsort` call.
///
/// Requires the argument be an indexed or associative array. Arity (exactly 1) is
/// pre-validated by the registry. Returns `Ok(PhpType::Void)` on success.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(cx.span, &format!("{}() argument must be array", cx.name)));
    }
    Ok(PhpType::Void)
}

/// Lowers a `natsort` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_natsort(ctx, inst)
}
