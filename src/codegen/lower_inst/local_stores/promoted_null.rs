//! Purpose:
//! Materializes absent boxed local storage before it becomes a persistent reference payload.
//!
//! Called from:
//! - Shared raw-local promotion for closures, aliases and by-reference foreach bindings.
//!
//! Key details:
//! - Missing raw slots use zero or NULL_SENTINEL; a Mixed reference needs a real boxed value.
//! - Installing the new owner in the raw slot preserves the promotion's retain/release balance.

use crate::codegen::context::FunctionContext;
use crate::codegen_support::{abi, platform::Arch};
use crate::types::PhpType;

/// Replaces only absent Mixed storage, leaving live boxes and native scalar payloads untouched.
pub(in crate::codegen::lower_inst) fn canonicalize_missing_mixed_slot(
    ctx: &mut FunctionContext<'_>, local_ty: &PhpType, offset: usize,
) {
    if local_ty.codegen_repr() != PhpType::Mixed { return; }
    let missing = ctx.next_label("promote_missing_mixed");
    let ready = ctx.next_label("promote_mixed_ready");
    let value = abi::int_result_reg(ctx.emitter);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_branch_if_zero_wide(ctx.emitter, value, &missing);
    abi::emit_load_int_immediate(ctx.emitter, scratch, crate::codegen_support::sentinels::NULL_SENTINEL);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cmp {value}, {scratch}")), // recognize unset storage before retaining its apparent pointer
        Arch::X86_64 => ctx.emitter.instruction(&format!("cmp {value}, {scratch}")), // recognize unset storage before retaining its apparent pointer
    }
    abi::emit_branch_if_not_equal_wide(ctx.emitter, &ready);
    ctx.emitter.label(&missing);
    crate::codegen::literal_defaults::emit_boxed_null_literal_to_result(ctx);
    abi::store_at_offset(ctx.emitter, value, offset);
    ctx.emitter.label(&ready);
}
