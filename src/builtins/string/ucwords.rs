//! Purpose:
//! Home of the PHP `ucwords` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - The declared signature carries the full golden param list (`string`, `separators`),
//!   but `max_args: 1` caps `check_arity` so a second argument is rejected, matching the
//!   legacy CHECK arm which enforced exactly one argument.
//! - No `check` hook is needed: the return type (`Str`) is fully determined by the
//!   declaration. The registry dispatch still infers each argument unconditionally, so
//!   undefined-variable diagnostics fire exactly as the legacy arm produced them.
//! - `lower` is a thin wrapper over the shared `lower_unary_string_runtime` emitter,
//!   passing the `__rt_ucwords` runtime helper.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "ucwords",
    area: String,
    params: [string: Str, separators: Str = crate::builtins::spec::DefaultSpec::Str(" \t\r\n\u{0c}\u{0b}")],
    max_args: 1,
    returns: Str,
    lower: lower,
    summary: "Uppercases the first character of each word in a string.",
    php_manual: "https://www.php.net/manual/en/function.ucwords.php",
}

/// Lowers a `ucwords` call by dispatching to the shared unary string-runtime emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_unary_string_runtime(
        ctx,
        inst,
        "ucwords",
        "__rt_ucwords",
    )
}
