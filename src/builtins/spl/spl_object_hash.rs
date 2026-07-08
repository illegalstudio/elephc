//! Purpose:
//! Home of the PHP `spl_object_hash` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required to validate that the argument is an object; returns `Str`.
//! - The hash is derived from the object's heap pointer stringified via `__rt_itoa`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "spl_object_hash",
    area: Spl,
    params: [object: Mixed],
    returns: Str,
    check: check,
    lower: lower,
    summary: "Return hash id for given object.",
    php_manual: "https://www.php.net/manual/en/function.spl-object-hash.php",
}

/// Validates that the argument is an object and returns `Str`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Object(_)) {
        return Err(CompileError::new(
            cx.span,
            "spl_object_hash() argument must be an object",
        ));
    }
    Ok(PhpType::Str)
}

/// Lowers `spl_object_hash()` by delegating to the object-pointer-to-string emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::spl::lower_spl_object_hash(ctx, inst)
}
