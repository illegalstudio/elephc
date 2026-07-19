//! Purpose:
//! Home of the PHP `spl_object_id` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required to validate that the argument is an object; returns `Int`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "spl_object_id",
    area: Spl,
    params: [object: Mixed],
    returns: Int,
    check: check,
    lower: lower,
    summary: "Return the integer object handle for given object.",
    php_manual: "https://www.php.net/manual/en/function.spl-object-id.php",
}

/// Validates that the argument is an object and returns `Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Object(_)) {
        return Err(CompileError::new(
            cx.span,
            "spl_object_id() argument must be an object",
        ));
    }
    Ok(PhpType::Int)
}

/// Lowers `spl_object_id()` by delegating to the object-pointer identity emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::spl::lower_spl_object_id(ctx, inst)
}
