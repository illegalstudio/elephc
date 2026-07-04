//! Purpose:
//! Home of the PHP `spl_classes` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required because the return type `Array<Str>` cannot be
//!   expressed as a plain `TypeSpec` ident in the `builtin!` macro.
//! - The function takes no arguments; arity is enforced by the registry.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "spl_classes",
    area: Spl,
    params: [],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Return available SPL classes.",
    php_manual: "https://www.php.net/manual/en/function.spl-classes.php",
}

/// Returns `Array<Str>` as the precise return type for `spl_classes()`.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Lowers `spl_classes()` by delegating to the static SPL class-name array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::spl::lower_spl_classes(ctx, inst)
}
