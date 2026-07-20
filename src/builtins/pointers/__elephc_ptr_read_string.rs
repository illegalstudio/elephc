//! Purpose:
//! Home of the internal `__elephc_ptr_read_string` builtin: the compiler-prelude
//! alias of `ptr_read_string`, sharing its check hook and lowering.
//!
//! Called from:
//! - Injected prelude PHP sources (`src/image_prelude.rs`, `src/web_prelude.rs`)
//!   through the builtin registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` (which hides
//!   PHP-visible extension builtins from user programs) does not affect it and
//!   prelude-injected code keeps compiling in strict mode.
//! - Delegates `check` and the lowering emitter to the `ptr_read_string` home so
//!   the two names cannot drift.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "__elephc_ptr_read_string",
    area: Pointers,
    params: [pointer: Mixed, length: Mixed],
    returns: Str,
    check: crate::builtins::pointers::ptr_read_string::check,
    lower: lower,
    summary: "Internal prelude alias of ptr_read_string.",
    internal: true,
}

/// Lowers an `__elephc_ptr_read_string` call through the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_ptr_read_string(ctx, inst)
}
