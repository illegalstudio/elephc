//! Purpose:
//! Home of the PHP `iterator_count` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required to validate that the argument is a statically known
//!   array or Traversable (not an arbitrary value); returns `Int`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;
use crate::types::checker::builtins::spl as checker_spl;

builtin! {
    name: "iterator_count",
    area: Spl,
    params: [iterator: Mixed],
    returns: Int,
    check: check,
    lower: lower,
    summary: "Count the elements in an iterator.",
    php_manual: "https://www.php.net/manual/en/function.iterator-count.php",
}

/// Validates the iterator source type and returns `Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    checker_spl::check_iterator_source(
        cx.checker,
        &cx.args[0],
        cx.span,
        cx.env,
        "iterator_count()",
    )?;
    Ok(PhpType::Int)
}

/// Lowers `iterator_count()` by delegating to the iterator-count emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::spl::lower_iterator_count(ctx, inst)
}
