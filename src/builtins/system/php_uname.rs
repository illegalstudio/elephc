//! Purpose:
//! Home of the PHP `php_uname` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the optional `mode` argument, when present, is a string type.
//! - `arity_error` overrides the default "takes at most 1 argument" message to match
//!   the legacy phrasing "takes 0 or 1 arguments".
//! - `lower` is a thin wrapper over `system::lower_php_uname` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "php_uname",
    area: System,
    params: [mode: Str = DefaultSpec::Str("a")],
    arity_error: "php_uname() takes 0 or 1 arguments",
    returns: Str,
    check: check,
    lower: lower,
    summary: "Returns information about the operating system PHP is running on.",
}

/// Validates that the optional `mode` argument is a string when present.
///
/// Returns `PhpType::Str` unconditionally; the error path fires when an argument
/// is provided but does not infer as `PhpType::Str`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(arg) = cx.args.first() {
        let ty = cx.checker.infer_type(arg, cx.env)?;
        if ty != PhpType::Str {
            return Err(CompileError::new(
                cx.span,
                "php_uname() argument must be string",
            ));
        }
    }
    Ok(PhpType::Str)
}

/// Lowers a `php_uname` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_php_uname(ctx, inst)
}
