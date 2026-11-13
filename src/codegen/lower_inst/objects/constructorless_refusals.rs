//! Purpose:
//! Preserves strict constructorless allocation errors for known abstract classes.
//!
//! Called from:
//! - The dynamic constructorless allocation entry before ordinary candidate lookup.
//!
//! Key details:
//! - Native SimpleXML factory allocation deliberately bypasses this strict guard.
//! - The saved class string is popped before the shared catchable Error emitter runs.

use super::*;

/// Rejects known abstract names before they can fall through as missing eval classes.
pub(super) fn emit_constructorless_abstract_refusals(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) {
    if matches!(inst.immediate, Some(Immediate::Bool(true))) { return; }
    let mut classes = ctx.module.class_infos.iter()
        .filter(|(_, info)| info.is_abstract)
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    classes.sort();
    if classes.is_empty() { return; }
    let done = ctx.next_label("constructorless_abstract_done");
    let labels = classes.iter().map(|class| {
        let label = ctx.next_label("constructorless_abstract_error");
        emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, class, &label);
        label
    }).collect::<Vec<_>>();
    abi::emit_jump(ctx.emitter, &done);
    for (class, label) in classes.iter().zip(&labels) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        super::super::exceptions::emit_error(ctx, &format!("Cannot instantiate abstract class {class}"));
    }
    ctx.emitter.label(&done);
}
