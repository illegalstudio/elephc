//! Purpose:
//! Selects writable native reference layouts without mutating a detached value snapshot.
//!
//! Called from:
//! - Local append and offset assignment lowering.
//!
//! Key details:
//! - Canonical references lend their current boxed child to the existing Mixed writer.
//! - Headerless native cells keep the concrete writer and its original ABI writeback.
//! - Detached locals use their frame storage, and by-value reads still own copies.

use super::*;

/// Selects a local native root for a deep store or append without evaluating its dimensions.
pub(super) fn lower_target(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, span: Span,
    operation: impl FnMut(&mut LoweringContext<'_, '_>),
) -> bool {
    let mut root = target;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind { root = array; }
    let ExprKind::Variable(name) = &root.kind else { return false; };
    lower(ctx, name, span, operation)
}

/// Runs an operation with the actual reference layout, preserving ordinary branch joins.
pub(in crate::ir_lower) fn lower(
    ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span,
    mut operation: impl FnMut(&mut LoweringContext<'_, '_>),
) -> bool {
    if ctx.live_native_mutation_locals.contains_key(name) { return false; }
    let Some(payload) = ctx.native_mutation_payload_type(name) else { return false; };
    let snapshot = ctx.load_local(name, Some(span));
    let layout = ctx.emit_value(Op::RuntimeCall, vec![snapshot.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ReferenceCellMutationLayout)),
        PhpType::Int, effects_lookup::runtime_effects(), Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, snapshot, Some(span));
    let frame_type = ctx.builder.local_php_type(ctx.local_slots[name]);
    if frame_type.codegen_repr() == payload.codegen_repr() {
        let boxed = layout_is(ctx, layout, 0, span);
        super::conditionals::lower_runtime_choice(ctx, boxed.value, span, |ctx, boxed| {
            lower_view(ctx, name, if boxed { PhpType::Mixed } else { payload.clone() }, &mut operation);
        });
        return true;
    }
    let attached = layout_is(ctx, layout, 1, span);
    super::conditionals::lower_runtime_choice(ctx, attached.value, span, |ctx, attached| {
        if attached {
            lower_view(ctx, name, payload.clone(), &mut operation);
        } else if frame_type.codegen_repr() == PhpType::Mixed {
            // Mixed frame storage cannot narrow later, so the detached case is already boxed.
            lower_view(ctx, name, PhpType::Mixed, &mut operation);
        } else {
            let detached = layout_is(ctx, layout, 2, span);
            super::conditionals::lower_runtime_choice(ctx, detached.value, span, |ctx, detached| {
                lower_view(ctx, name, if detached { frame_type.clone() } else { PhpType::Mixed },
                    &mut operation);
            });
        }
    });
    true
}

/// Tests a layout code without using a value snapshot as the mutation receiver.
fn layout_is(ctx: &mut LoweringContext<'_, '_>, layout: LoweredValue, code: i64, span: Span) -> LoweredValue {
    let expected = ctx.builder.emit_const_i64(code);
    ctx.emit_value(Op::ICmp, vec![layout.value, expected],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)), PhpType::Bool,
        Op::ICmp.default_effects(), Some(span))
}

/// Limits a concrete or boxed writable view to this operation's explicit LHS loads.
fn lower_view(
    ctx: &mut LoweringContext<'_, '_>, name: &str, view: PhpType,
    operation: &mut impl FnMut(&mut LoweringContext<'_, '_>),
) {
    ctx.set_local_logical_type(name, view.clone());
    ctx.live_native_mutation_locals.insert(name.to_string(), view);
    operation(ctx);
    ctx.live_native_mutation_locals.remove(name);
    // The entry declaration remains a physical contract, not a later value fact.
    ctx.set_local_logical_type(name, PhpType::Mixed);
}
