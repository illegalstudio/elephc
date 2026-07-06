//! Purpose:
//! Home of the PHP `hash_equals` builtin: declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook is needed: `returns: Bool` expresses the return type inline and no
//!   bridge library is required (this is a pure timing-safe byte comparison).
//! - Arity (exactly 2 args) is validated by the registry.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "hash_equals",
    area: String,
    params: [known_string: Str, user_string: Str],
    returns: Bool,
    lower: lower,
    summary: "Compares two strings using a constant-time algorithm.",
    php_manual: "https://www.php.net/manual/en/function.hash-equals.php",
}

/// Lowers a `hash_equals` call by dispatching to the shared `lower_hash_equals` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_hash_equals(ctx, inst)
}
