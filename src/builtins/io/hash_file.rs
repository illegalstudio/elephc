//! Purpose:
//! Home of the PHP `hash_file` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, Bool)` reflecting PHP behaviour where `hash_file`
//!   returns the digest string or `false` when the file cannot be read.
//! - The `check` hook links `elephc_crypto`: `hash_file` reads the file then hashes
//!   through the crypto bridge (full algorithm set, raw `$binary` output).
//! - `lower` is a thin wrapper over `io::lower_hash_file` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "hash_file",
    area: Io,
    params: [algo: Str, filename: Str, binary: Bool = DefaultSpec::Bool(false)],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Generates a hash value using the contents of a given file.",
    php_manual: "function.hash-file",
}

/// Returns `Union(Str, Bool)` and links `elephc_crypto` for the digest routine.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    cx.checker.require_builtin_library("elephc_crypto");
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}

/// Lowers a `hash_file` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_hash_file(ctx, inst)
}
