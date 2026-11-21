//! Purpose:
//! Tests whether a pending compound append still has a live PHP array owner.
//!
//! Called from:
//! - Typed runtime-call dispatch for MixedArrayCellHasLiveOwner.
//!
//! Key details:
//! - The operand borrows one ordinary cell lease, not a PHP reference wrapper.
//! - Exactly one surviving PHP owner keeps the operation active. Cell and payload
//!   counts each include one compiler-held owner, so their sum must be three.
//! - Compiler-only ancestor leases must be retired before this non-retaining query.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

/// Reads cell and array/hash ownership without callbacks or PHP-visible comparisons.
pub(super) fn lower(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ctx.load_value_to_result(super::expect_operand(inst, 0)?)?;
    let payload = ctx.next_label("append_live_array_payload");
    let live = ctx.next_label("append_array_live");
    let done = ctx.next_label("append_array_liveness_done");
    elephc_memoir::heap_refcount::emit_boxed_array_liveness(ctx.emitter, &payload, &live, &done);
    super::store_if_result(ctx, inst)
}
