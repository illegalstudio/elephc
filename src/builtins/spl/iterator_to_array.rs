//! Purpose:
//! Home of the PHP `iterator_to_array` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required because the return type depends on the source type and
//!   the `preserve_keys` argument (static bool narrows to `AssocArray` or `Array`).
//! - The `returns: Mixed` macro field is a conservative fallback; the check hook always
//!   returns the precise array type.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;
use crate::types::checker::builtins::spl as checker_spl;

builtin! {
    name: "iterator_to_array",
    area: Spl,
    params: [iterator: Mixed, preserve_keys: Bool = DefaultSpec::Bool(true)],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Copy the iterator into an array.",
    php_manual: "https://www.php.net/manual/en/function.iterator-to-array.php",
}

/// Validates the source and computes the precise array return type based on `preserve_keys`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let source_ty = checker_spl::check_iterator_source(
        cx.checker,
        &cx.args[0],
        cx.span,
        cx.env,
        "iterator_to_array()",
    )?;
    let preserve_keys = if let Some(arg) = cx.args.get(1) {
        checker_spl::check_iterator_to_array_preserve_keys(cx.checker, arg, cx.env)?
    } else {
        Some(true)
    };
    Ok(checker_spl::iterator_to_array_return_type(
        cx.checker,
        &source_ty,
        preserve_keys,
    ))
}

/// Lowers `iterator_to_array()` by delegating to the iterator-to-array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::spl::lower_iterator_to_array(ctx, inst)
}
