//! Purpose:
//! Home of the PHP `strstr` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - The declared signature carries the full golden param list (`haystack`, `needle`,
//!   `before_needle`), but `max_args: 2` caps `check_arity` so a third argument is
//!   rejected, matching the legacy CHECK arm which enforced exactly two arguments.
//! - No `check` hook is needed: the return type (`Str`) is fully determined by the
//!   declaration. The registry dispatch still infers each argument unconditionally, so
//!   undefined-variable diagnostics fire exactly as the legacy arm produced them.
//! - `lower` is a thin wrapper over the shared `lower_strstr` emitter.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "strstr",
    area: String,
    params: [haystack: Str, needle: Str, before_needle: Bool = crate::builtins::spec::DefaultSpec::Bool(false)],
    max_args: 2,
    returns: Str,
    lower: lower,
    summary: "Returns the portion of a string starting at the first occurrence of a substring.",
    php_manual: "https://www.php.net/manual/en/function.strstr.php",
}

/// Lowers a `strstr` call by dispatching to the shared `lower_strstr` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_strstr(ctx, inst)
}
