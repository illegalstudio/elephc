//! Purpose:
//! Preserves strict constructorless allocation errors for known abstract classes and the
//! contract's internal-final classes.
//!
//! Called from:
//! - The dynamic constructorless allocation entry before ordinary candidate lookup.
//!
//! Key details:
//! - Native SimpleXML factory allocation deliberately bypasses this strict guard.
//! - The saved class string is popped before the shared catchable error emitters run.
//! - Internal-final classes must be refused here as well as in the statically known path:
//!   a nullsafe/dynamic `ReflectionClass` call has no compile-time class name.

use super::*;

/// Rejects known abstract and internal-final names before they can fall through as missing
/// eval classes or allocate an object with an uninitialized native handle.
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
    let mut internal_finals = elephc_builtin_contract::INTERNAL_FINAL_CLASSES
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    internal_finals.sort();
    if classes.is_empty() && internal_finals.is_empty() { return; }
    let done = ctx.next_label("constructorless_refusal_done");
    let labels = classes.iter().map(|class| {
        let label = ctx.next_label("constructorless_abstract_error");
        emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, class, &label);
        label
    }).collect::<Vec<_>>();
    let final_labels = internal_finals.iter().map(|class| {
        let label = ctx.next_label("constructorless_internal_final_error");
        emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, class, &label);
        label
    }).collect::<Vec<_>>();
    abi::emit_jump(ctx.emitter, &done);
    for (class, label) in classes.iter().zip(&labels) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        super::super::exceptions::emit_error(ctx, &format!("Cannot instantiate abstract class {class}"));
    }
    for (class, label) in internal_finals.iter().zip(&final_labels) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        super::super::exceptions::emit_reflection_exception_at(
            ctx,
            &format!(
                "Class {class} is an internal class marked as final that cannot be instantiated without invoking its constructor"
            ),
            None,
        );
    }
    ctx.emitter.label(&done);
}
