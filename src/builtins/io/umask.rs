//! Purpose:
//! Home of the PHP `umask` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `umask` is a pure-data builtin whose `Int` return type is
//!   fully determined by its declaration. The registry common path infers the
//!   optional argument and enforces arity before falling back to `returns`.
//! - `arity_error` is overridden to preserve the legacy message
//!   "umask() takes 0 or 1 arguments" (the registry default for a 0-required,
//!   1-optional builtin produces "takes at most 1 argument").
//! - `lower` is a thin wrapper over `io::lower_umask` in the EIR backend.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "umask",
    area: Io,
    params: [mask: Int = DefaultSpec::Null],
    arity_error: "umask() takes 0 or 1 arguments",
    returns: Int,
    lower: lower,
    summary: "Changes the current umask.",
    php_manual: "function.umask",
}

/// Lowers a `umask` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_umask(ctx, inst)
}
