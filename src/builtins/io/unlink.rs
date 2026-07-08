//! Purpose:
//! Home of the PHP `unlink` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool`. Unlike `mkdir`/`rmdir`/`chdir`, `unlink` carries a PHAR
//!   side effect: a literal `phar://` URL or any non-literal path links `elephc_phar`
//!   because deletion may target an entry inside a PHAR archive.
//! - `lower` is a thin wrapper over `io::lower_unlink` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "unlink",
    area: Io,
    params: [filename: Str],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Deletes a file.",
    php_manual: "function.unlink",
}

/// Returns `Bool` and links `elephc_phar` when the target may live in a PHAR archive.
///
/// A literal `phar://` URL links `elephc_phar`; a non-literal path also links it
/// because the scheme is unknown at compile time. A literal non-`phar://` path
/// needs no PHAR bridge.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(ExprKind::StringLiteral(url)) = cx.args.first().map(|a| &a.kind) {
        if url.starts_with("phar://") {
            cx.checker.require_builtin_library("elephc_phar");
        }
    } else {
        cx.checker.require_builtin_library("elephc_phar");
    }
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Bool)
}

/// Lowers an `unlink` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_unlink(ctx, inst)
}
