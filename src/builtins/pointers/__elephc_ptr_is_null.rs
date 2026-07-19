//! Purpose:
//! Home of the internal `__elephc_ptr_is_null` builtin: the compiler-prelude
//! alias of `ptr_is_null`, sharing its check hook and lowering.
//!
//! Called from:
//! - Injected prelude PHP sources (`src/image_prelude.rs`) through the builtin
//!   registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` (which hides
//!   PHP-visible extension builtins from user programs) does not affect it and
//!   prelude-injected code keeps compiling in strict mode.
//! - Delegates `check` and the lowering emitter to the `ptr_is_null` home so the
//!   two names cannot drift.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "__elephc_ptr_is_null",
    area: Pointers,
    params: [pointer: Mixed],
    returns: Bool,
    check: crate::builtins::pointers::ptr_is_null::check,
    lower: lower,
    summary: "Internal prelude alias of ptr_is_null.",
    internal: true,
}

/// Lowers an `__elephc_ptr_is_null` call through the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_ptr_is_null(ctx, inst)
}
