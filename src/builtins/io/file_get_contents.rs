//! Purpose:
//! Home of the PHP `file_get_contents` builtin: its declaration, type-check hook,
//! and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, Bool)` reflecting PHP behaviour where the read
//!   returns the file contents or `false` on failure.
//! - The `check` hook has a library-linking side effect: a literal `https://` /
//!   `ftps://` URL links `elephc_tls`; a non-literal path conservatively links
//!   `elephc_tls`, `elephc_phar`, `z`, and `bz2` because the scheme and PHAR entry
//!   flags are unknown until run time.
//! - `lower` is a thin wrapper over `io::lower_file_get_contents` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "file_get_contents",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Reads an entire file into a string.",
    php_manual: "function.file-get-contents",
}

/// Returns `Union(Str, Bool)` and records the runtime libraries the call may need.
///
/// A literal `https://`/`ftps://` URL is read over TLS, so it links `elephc_tls`.
/// A non-literal path routes through the runtime URL dispatcher, whose scheme and
/// PHAR entry flags are unknown at compile time, so it conservatively links TLS
/// plus the PHAR bridge and decompression libraries (`z`, `bz2`).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(ExprKind::StringLiteral(url)) = cx.args.first().map(|a| &a.kind) {
        if url.starts_with("https://") || url.starts_with("ftps://") {
            cx.checker.require_builtin_library("elephc_tls");
        }
    } else {
        cx.checker.require_builtin_library("elephc_tls");
        cx.checker.require_builtin_library("elephc_phar");
        cx.checker.require_builtin_library("z");
        cx.checker.require_builtin_library("bz2");
    }
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::Bool]))
}

/// Lowers a `file_get_contents` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_file_get_contents(ctx, inst)
}
