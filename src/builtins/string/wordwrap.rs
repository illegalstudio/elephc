//! Purpose:
//! Home of the PHP `wordwrap` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   all via `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts a required `string` param plus optional `width`, `break`, and
//!   `cut_long_words` params with PHP-compatible defaults. The `break` param
//!   uses the raw identifier `r#break` because `break` is a Rust keyword.
//! - `lower` is a thin wrapper over the shared `lower_wordwrap` emitter.

use crate::builtins::spec::DefaultSpec;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "wordwrap",
    area: String,
    params: [
        string: Str,
        width: Int = DefaultSpec::Int(75),
        r#break: Str = DefaultSpec::Str("\n"),
        cut_long_words: Bool = DefaultSpec::Bool(false)
    ],
    returns: Str,
    lower: lower,
    summary: "Wraps a string to a given number of characters.",
    php_manual: "https://www.php.net/manual/en/function.wordwrap.php",
}

/// Lowers a `wordwrap` call by dispatching to the shared wordwrap emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_wordwrap(ctx, inst)
}
