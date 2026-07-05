//! Purpose:
//! Home of the PHP `fnmatch` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the optional `flags` argument, when present, has type `Int`.
//! - The registry pre-infers all arguments before calling the hook; the hook calls
//!   `infer_type` on `flags` again (idempotent) to obtain its resolved type.
//! - `lower` is a thin wrapper over `io::lower_fnmatch` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "fnmatch",
    area: Io,
    params: [pattern: Str, filename: Str, flags: Int = DefaultSpec::Int(0)],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Matches a filename against a pattern.",
    php_manual: "function.fnmatch",
}

/// Returns `Bool`, requiring the optional `flags` argument to be of type `Int`.
///
/// The registry pre-infers all arguments before calling this hook. The hook
/// re-infers the optional `flags` argument (idempotent) to obtain its resolved
/// type, and emits a diagnostic if the type is not `Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(flags) = cx.args.get(2) {
        let flags_ty = cx.checker.infer_type(flags, cx.env)?;
        if flags_ty != PhpType::Int {
            return Err(CompileError::new(cx.span, "fnmatch() flags must be int"));
        }
    }
    Ok(PhpType::Bool)
}

/// Lowers a `fnmatch` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_fnmatch(ctx, inst)
}
