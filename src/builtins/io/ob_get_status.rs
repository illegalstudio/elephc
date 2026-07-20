//! Purpose:
//! Home of the PHP `ob_get_status` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook when present),
//!   and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `AssocArray<Mixed, Mixed>`: simple mode yields the top
//! -   buffer's status (string keys), full mode a list of per-level status arrays.
//! - Every entry reports the default output handler (user handlers unsupported).
//! - `lower` is a thin wrapper over `output_buffering::lower_ob_get_status`.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "ob_get_status",
    area: Io,
    params: [full_status: Bool = DefaultSpec::Bool(false)],
    returns: Mixed,
    returns_fresh_storage: true,
    check: check,
    lower: lower,
    summary: "Gets status of output buffers.",
    php_manual: "function.ob-get-status",
}

/// Returns `AssocArray<Mixed, Mixed>`: string-keyed status fields in simple mode,
/// an int-keyed list of per-level status arrays in full mode.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    })
}

/// Lowers an `ob_get_status` call by dispatching to the shared output-buffering emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::output_buffering::lower_ob_get_status(ctx, inst)
}
