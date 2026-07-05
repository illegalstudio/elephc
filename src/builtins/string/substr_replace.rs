//! Purpose:
//! Home of the PHP `substr_replace` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   all via `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts required `string`, `replace`, and `offset` params, plus an optional
//!   `length` param defaulting to null.
//! - `lower` is a thin wrapper over the shared `lower_substr_replace` emitter.

use crate::builtins::spec::DefaultSpec;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "substr_replace",
    area: String,
    params: [string: Str, replace: Str, offset: Int, length: Mixed = DefaultSpec::Null],
    returns: Str,
    lower: lower,
    summary: "Replaces text within a portion of a string.",
    php_manual: "https://www.php.net/manual/en/function.substr-replace.php",
}

/// Lowers a `substr_replace` call by dispatching to the shared substr-replace emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_substr_replace(ctx, inst)
}
