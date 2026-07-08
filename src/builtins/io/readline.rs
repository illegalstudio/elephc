//! Purpose:
//! Home of the PHP `readline` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, Bool)` to match PHP's false-on-failure pattern for
//!   end-of-input. The `prompt` argument is optional and pre-inferred by the registry.
//! - `arity_error` is overridden to "readline() takes 0 or 1 arguments" because the
//!   registry's default message for min0/max1 ("takes at most 1 argument") does not
//!   match the legacy error text.
//! - `returns: Mixed` is used because the union cannot be expressed through the scalar
//!   `returns:` field.
//! - `lower` is a thin wrapper over `io::lower_readline` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "readline",
    area: Io,
    params: [prompt: Str = DefaultSpec::Null],
    arity_error: "readline() takes 0 or 1 arguments",
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Reads a line from the user's terminal.",
    php_manual: "function.readline",
}

/// Returns `Union(Str, Bool)` for the readline result (false on end-of-input).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::Str,
        PhpType::Bool,
    ]))
}

/// Lowers a `readline` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_readline(ctx, inst)
}
