//! Purpose:
//! Home of the internal `__elephc_ptr_write_string` builtin: the compiler-prelude
//! alias of `ptr_write_string`, sharing its check hook and lowering.
//!
//! Called from:
//! - Injected prelude PHP sources (`src/image_prelude.rs`) through the builtin
//!   registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` (which hides
//!   PHP-visible extension builtins from user programs) does not affect it and
//!   prelude-injected code keeps compiling in strict mode.
//! - Delegates `check` and the lowering emitter to the `ptr_write_string` home so
//!   the two names cannot drift.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "__elephc_ptr_write_string",
    area: Pointers,
    params: [pointer: Mixed, string: Mixed],
    returns: Int,
    check: crate::builtins::pointers::ptr_write_string::check,
    lower: lower,
    summary: "Internal prelude alias of ptr_write_string.",
    internal: true,
}

/// Lowers an `__elephc_ptr_write_string` call through the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_ptr_write_string(ctx, inst)
}
