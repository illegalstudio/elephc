//! Purpose:
//! Joins compiler-selected runtime alternatives with the ordinary conditional storage protocol.
//!
//! Called from:
//! - Native reference mutation layout dispatch.
//!
//! Key details:
//! - Each arm starts with the same local, callable, reflection and alias facts.
//! - Reachable exits reuse the normal type reconciliation and initialization intersection.

use super::*;
use crate::ir::ValueId;

/// Lowers one runtime-selected operation per path without leaking one arm's facts into the other.
pub(in crate::ir_lower::stmt) fn lower_runtime_choice(
    ctx: &mut LoweringContext<'_, '_>, condition: ValueId, span: Span,
    mut operation: impl FnMut(&mut LoweringContext<'_, '_>, bool),
) {
    let selected = ctx.builder.create_named_block("native.mutation.raw", Vec::new());
    let alternate = ctx.builder.create_named_block("native.mutation.boxed", Vec::new());
    let merge = ctx.builder.create_named_block("native.mutation.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: condition,
        then_target: selected, then_args: Vec::new(), else_target: alternate, else_args: Vec::new() });
    let initialized = ctx.initialized_slots_snapshot();
    let types = ctx.local_types_snapshot();
    let callables = ctx.static_callable_locals_snapshot();
    let reflection = ctx.reflection_bindings_snapshot();
    let borrowed = ctx.borrowed_element_ref_locals_snapshot();
    let markers = ctx.reference_marker_array_locals_snapshot();
    let mut arms = Vec::new();
    ctx.builder.position_at_end(selected);
    operation(ctx, true);
    let selected_initialized = ctx.initialized_slots_snapshot();
    let selected_borrowed = ctx.borrowed_element_ref_locals_snapshot();
    let selected_markers = ctx.reference_marker_array_locals_snapshot();
    let selected_reachable = !ctx.builder.insertion_block_is_terminated();
    if selected_reachable { record_if_arm_exit(ctx, &mut arms); }
    ctx.builder.position_at_end(alternate);
    ctx.restore_initialized_slots(initialized.clone());
    ctx.restore_local_types(types);
    ctx.restore_static_callable_locals(callables);
    ctx.restore_reflection_bindings(reflection);
    ctx.restore_borrowed_element_ref_locals(borrowed);
    ctx.restore_reference_marker_array_locals(markers);
    operation(ctx, false);
    let alternate_initialized = ctx.initialized_slots_snapshot();
    let alternate_reachable = !ctx.builder.insertion_block_is_terminated();
    if alternate_reachable { record_if_arm_exit(ctx, &mut arms); }
    else {
        ctx.restore_borrowed_element_ref_locals(HashSet::new());
        ctx.restore_reference_marker_array_locals(HashSet::new());
    }
    if selected_reachable {
        ctx.merge_borrowed_element_ref_locals(&selected_borrowed);
        ctx.merge_reference_marker_array_locals(&selected_markers);
    }
    ctx.restore_initialized_slots(merge_initialized_slots(&initialized,
        selected_initialized, selected_reachable, alternate_initialized, alternate_reachable));
    finish_if_type_join(ctx, arms, merge, span);
    ctx.builder.position_at_end(merge);
    if !selected_reachable && !alternate_reachable { ctx.builder.terminate(Terminator::Unreachable); }
    let callables = ctx.static_callable_locals_snapshot();
    let reflection = ctx.reflection_bindings_snapshot();
    ctx.clear_static_callable_locals();
    ctx.restore_static_callable_locals(callables);
    ctx.restore_reflection_bindings(reflection);
}
