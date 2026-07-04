//! Purpose:
//! Home of the PHP `file_put_contents` builtin: its declaration, type-check hook,
//! and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Int` (the number of bytes written).
//! - The `check` hook links the PHAR bridge: a literal `phar://` URL writes through
//!   the read-modify-write bridge and links `elephc_phar` plus `elephc_crypto` (the
//!   assembly SHA1 path remains a fallback); any non-literal path links `elephc_phar`.
//! - `lower` is a thin wrapper over `io::lower_file_put_contents` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "file_put_contents",
    area: Io,
    params: [filename: Str, data: Str],
    returns: Int,
    check: check,
    lower: lower,
    summary: "Writes data to a file.",
    php_manual: "function.file-put-contents",
}

/// Returns `Int` and records the PHAR libraries the write may need.
///
/// A literal `phar://` target writes through the `elephc_phar` bridge and also links
/// `elephc_crypto`; any other target (including non-literal paths) links `elephc_phar`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(ExprKind::StringLiteral(url)) = cx.args.first().map(|a| &a.kind) {
        if url.starts_with("phar://") {
            cx.checker.require_builtin_library("elephc_phar");
            cx.checker.require_builtin_library("elephc_crypto");
        }
    } else {
        cx.checker.require_builtin_library("elephc_phar");
    }
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    Ok(PhpType::Int)
}

/// Lowers a `file_put_contents` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_file_put_contents(ctx, inst)
}
