//! Purpose:
//! Home of the PHP `clearstatcache` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `clearstatcache` is a pure-data builtin whose return
//!   type (`Void`) is fully determined by its declaration. The registry common path
//!   infers arguments and enforces arity before falling back to `returns`.
//! - PHP accepts up to 2 optional arguments; elephc has no stat cache but accepts
//!   and ignores them (matching legacy behavior).
//! - `lower` is a thin wrapper over `io::lower_clearstatcache` in the EIR backend.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "clearstatcache",
    area: Io,
    params: [
        clear_realpath_cache: Bool = DefaultSpec::Bool(false),
        filename: Str = DefaultSpec::Str("")
    ],
    returns: Void,
    lower: lower,
    summary: "Clears file status cache.",
    php_manual: "function.clearstatcache",
}

/// Lowers a `clearstatcache` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_clearstatcache(ctx, inst)
}
