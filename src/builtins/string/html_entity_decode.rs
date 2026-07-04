//! Purpose:
//! Home of the PHP `html_entity_decode` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `html_entity_decode` is a pure-data builtin whose
//!   return type (`Str`) is fully determined by its declaration. The registry derives
//!   the return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over the shared `lower_unary_string_runtime` emitter,
//!   passing the `__rt_html_entity_decode` runtime helper.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "html_entity_decode",
    area: String,
    params: [string: Str],
    returns: Str,
    lower: lower,
    summary: "Converts HTML entities in a string back into their corresponding characters.",
    php_manual: "https://www.php.net/manual/en/function.html-entity-decode.php",
}

/// Lowers a `html_entity_decode` call by dispatching to the shared per-arch unary string runtime.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_unary_string_runtime(
        ctx,
        inst,
        "html_entity_decode",
        "__rt_html_entity_decode",
    )
}
