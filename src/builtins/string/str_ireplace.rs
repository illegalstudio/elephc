//! Purpose:
//! Home of the PHP `str_ireplace` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   all via `crate::builtins::registry`.
//!
//! Key details:
//! - The declared signature includes an optional `count` param, but `max_args: 3`
//!   caps arity so only three arguments are accepted, matching PHP's practical use.
//! - `lower` is a thin wrapper over the shared `lower_string_replace` emitter.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "str_ireplace",
    area: String,
    params: [search: Str, replace: Str, subject: Str, count: Mixed = DefaultSpec::Null],
    max_args: 3,
    returns: Str,
    lower: lower,
    summary: "Case-insensitive version of str_replace().",
    php_manual: "https://www.php.net/manual/en/function.str-ireplace.php",
}

/// Lowers a `str_ireplace` call by dispatching to the shared string-replace emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_string_replace(
        ctx,
        inst,
        "str_ireplace",
        "__rt_str_ireplace",
    )
}
