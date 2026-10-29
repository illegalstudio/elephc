//! Purpose:
//! Autovivifies nested append buckets exposed through boxed array properties.
//!
//! Called from:
//! - The shared nested-append planner after evaluating and rooting the RHS.
//!
//! Key details:
//! - Runtime array guards preserve the ordinary fallback for non-array property values.
//! - The unboxed probe owner remains published while diagnostics and writeback can throw.

use super::*;
use crate::ir::{Immediate, PhpTypePredicate, Terminator};

/// Uses the ordinary array probe for a boxed array while preserving other receiver protocols.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, group: &NestedAppendGroup<'_>, container: LoweredValue, span: Span,
) {
    let array = ctx.emit_value(Op::TypePredicate, vec![container.value],
        Some(Immediate::TypePredicate(PhpTypePredicate::Array)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let yes = ctx.builder.create_named_block("napp.boxed_array", Vec::new());
    let no = ctx.builder.create_named_block("napp.boxed_fallback", Vec::new());
    let done = ctx.builder.create_named_block("napp.boxed_done", Vec::new());
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: array.value, then_target: yes, then_args: Vec::new(),
        else_target: no, else_args: Vec::new() });

    ctx.builder.position_at_end(yes);
    let raw = ctx.emit_value(Op::MixedUnbox, vec![container.value], None,
        PhpType::Array(Box::new(PhpType::Mixed)), Op::MixedUnbox.default_effects(), Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, container, Some(span));
    let (raw, owner) = super::super::nested_array_writes::root_static_update_receiver(ctx, raw, span);
    lower_from_container(ctx, group, raw, span);
    let yes_reaches = !ctx.builder.insertion_block_is_terminated();
    if yes_reaches {
        if let Some(owner) = owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
        super::super::branch_to(ctx, done);
    }
    let yes_slots = ctx.initialized_slots_snapshot();

    ctx.builder.position_at_end(no);
    ctx.restore_initialized_slots(initial.clone());
    crate::ir_lower::ownership::release_if_owned(ctx, container, Some(span));
    super::super::lower_stmt(ctx, group.read);
    super::super::lower_stmt(ctx, group.push);
    super::super::lower_stmt(ctx, group.write_back);
    let no_reaches = !ctx.builder.insertion_block_is_terminated();
    if no_reaches { super::super::branch_to(ctx, done); }
    let no_slots = ctx.initialized_slots_snapshot();

    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(super::super::merge_initialized_slots(
        &initial, yes_slots, yes_reaches, no_slots, no_reaches,
    ));
}
