//! Purpose:
//! Home of the PHP `crc32` builtin: declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), both via
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook needed: `returns: Int` expresses the return type inline and no
//!   bridge library is required (crc32 is a pure table-free computation in __rt_crc32).
//! - Arity (exactly 1 arg) is validated by the registry.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "crc32",
    area: String,
    params: [string: Str],
    returns: Int,
    lower: lower,
    summary: "Calculates the CRC32 polynomial of a string.",
    php_manual: "https://www.php.net/manual/en/function.crc32.php",
}

/// Lowers a `crc32` call by dispatching to the shared `lower_crc32` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_crc32(ctx, inst)
}
