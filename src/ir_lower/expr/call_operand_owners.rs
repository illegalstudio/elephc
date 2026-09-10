//! Purpose:
//! Roots owned expression operands across calls that can unwind out of their caller.
//!
//! Called from:
//! - User calls, descriptor invocation, callback builtins and eval call lowering.
//!
//! Key details:
//! - Scoped unwind records retire roots before same-frame catches; frame cleanup sees cleared slots.
//! - Borrowed local loads remain subject to final storage-aware release pruning.

use super::*;

/// Starts a nested source-argument evaluation ledger.
pub(super) fn begin_call_argument_evaluation(ctx: &mut LoweringContext<'_, '_>) {
    ctx.call_argument_evaluation_scopes.push(
        crate::ir_lower::context::CallArgumentEvaluationScope {
            expression_depth: ctx.expression_depth,
            owners: Vec::new(),
        },
    );
}

/// Transfers an owned by-value argument into an unwind-visible slot before later evaluation.
///
/// The exposed `Borrow` preserves the producer chain specialized consumers inspect without
/// duplicating the slot's lease. Successful lowering later hands the stored Acquire SSA to the
/// final operand or keeps the slot published until an intermediate can retire safely.
pub(super) fn root_evaluated_call_argument(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if !ctx
        .call_argument_evaluation_scopes
        .last()
        .is_some_and(|scope| scope.expression_depth == ctx.expression_depth)
        || !ctx.value_needs_release_after_use(value)
    {
        return value;
    }
    let ty = ctx.builder.value_php_type(value.value);
    if matches!(ty.codegen_repr(), PhpType::Buffer(_)) {
        return value;
    }
    let temp_name = ctx.declare_owned_hidden_temp(ty.clone());
    let rooted =
        crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, value, Some(span));
    ctx.store_local(&temp_name, rooted, ty, Some(span));
    let slot = ctx.local_slots[&temp_name];
    register_owned_call_operand(ctx, slot, span);
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    let borrowed_ty = ctx.builder.value_php_type(rooted.value);
    let borrowed = ctx
        .builder
        .emit_with_effects(
            Op::Borrow,
            vec![rooted.value],
            None,
            rooted.ir_type,
            borrowed_ty,
            Ownership::Borrowed,
            Op::Borrow.default_effects(),
            Some(span),
        )
        .expect("call argument evaluation borrow produces a value");
    ctx.call_argument_evaluation_scopes
        .last_mut()
        .expect("call argument evaluation scope")
        .owners
        .push(crate::ir_lower::context::CallArgumentEvaluationOwner {
            value: rooted.value,
            borrow: borrowed,
            temp_name,
            slot,
            span,
        });
    LoweredValue {
        value: borrowed,
        ir_type: rooted.ir_type,
    }
}

/// Returns whether a final call operand is the lease transferred from source evaluation.
///
/// The lifetime-pin marker distinguishes these operands from ordinary call roots. A
/// may-alias string runtime can return a view into this lease, so it must make that view
/// independent before the ordinary argument cleanup retires the pinned allocation.
pub(super) fn value_is_call_argument_evaluation_pin(
    ctx: &LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
) -> bool {
    ctx.builder
        .value_defining_instruction(value)
        .is_some_and(|inst| {
            inst.op == Op::Acquire && inst.immediate == Some(Immediate::Bool(true))
        })
}

/// Hands final operands their stored owner and republishes intermediate roots through the call.
pub(super) fn finish_call_argument_evaluation(
    ctx: &mut LoweringContext<'_, '_>,
    operands: &mut [crate::ir::ValueId],
) -> Vec<(crate::ir::LocalSlotId, Span)> {
    let scope = ctx
        .call_argument_evaluation_scopes
        .pop()
        .expect("call argument evaluation scope must be balanced");
    debug_assert_eq!(scope.expression_depth, ctx.expression_depth);
    let mut intermediates = Vec::new();
    for owner in scope.owners.into_iter().rev() {
        unregister_owned_call_operand(ctx, owner.slot, owner.span);
        let mut retained_by_call = false;
        for operand in operands.iter_mut() {
            if *operand == owner.borrow {
                *operand = owner.value;
                retained_by_call = true;
            } else if *operand == owner.value {
                retained_by_call = true;
            }
        }
        if retained_by_call {
            ctx.clear_owned_hidden_temp(&owner.temp_name, Some(owner.span));
        } else {
            intermediates.push((owner.slot, owner.span));
        }
    }
    intermediates.reverse();
    for (slot, span) in &intermediates {
        register_owned_call_operand(ctx, *slot, *span);
    }
    intermediates
}

/// Retires source-evaluation leases after the enclosing call's ordinary cleanup.
pub(super) fn retire_call_argument_intermediates(
    ctx: &mut LoweringContext<'_, '_>,
    roots: &[(crate::ir::LocalSlotId, Span)],
) {
    for (slot, span) in roots.iter().rev() {
        retire_owned_call_operand(ctx, *slot, *span);
    }
}

/// Roots callback, validation and XML-setter operands with independently owned results.
pub(super) fn root_non_aliasing_callback_operands(
    ctx: &mut LoweringContext<'_, '_>,
    def: &crate::builtins::registry::BuiltinDef,
    operands: &mut [crate::ir::ValueId],
    result_type: &PhpType,
    span: Span,
) -> Vec<(usize, crate::ir::LocalSlotId)> {
    use crate::builtins::semantics::{BuiltinArgumentLowering, BuiltinLowering, BuiltinResultOwnership};
    use crate::ir::RuntimeCallTarget;
    let needs_unwind_roots = match def.spec.semantics.lowering {
        BuiltinLowering::Runtime(RuntimeCallTarget::Function(target)
            | RuntimeCallTarget::ProfiledFunction { target, .. }) => {
            target.string_callback_operand_index().is_some()
                // Aggregates enter user code through warning handlers rather than
                // an explicit callback operand. Their internal snapshot does not
                // own the original temporary passed by this PHP activation.
                || matches!(target, crate::ir::RuntimeFnId::ArraySum | crate::ir::RuntimeFnId::ArrayProduct)
                // Descriptor merge wrappers extract owned Mixed cells from their variadic
                // pack. Invalid runtime tags throw before ordinary post-call releases.
                || target == crate::ir::RuntimeFnId::ArrayMerge
        }
        _ => matches!(
            def.spec.semantics.argument_lowering,
            BuiltinArgumentLowering::XmlHandlerSetter,
        ),
    };
    let independent_result = !crate::ir::Ownership::php_type_needs_lifetime_tracking(result_type)
        || matches!(
            def.spec.semantics.result_ownership,
            BuiltinResultOwnership::NonHeap | BuiltinResultOwnership::Fresh | BuiltinResultOwnership::Independent
        );
    if !needs_unwind_roots || !independent_result {
        return Vec::new();
    }
    let mut roots = Vec::new();
    for (index, operand) in operands.iter_mut().enumerate() {
        if def.ref_params.get(index).copied().unwrap_or(false) { continue; }
        let value = LoweredValue { value: *operand, ir_type: ctx.builder.value_type(*operand) };
        let (value, root) = root_owned_call_operand(ctx, value, span);
        *operand = value.value;
        if let Some(slot) = root { roots.push((index, slot)); }
    }
    roots
}

/// Roots value arguments and fresh container defaults before the callee may unwind.
/// A result-independent call or a separately owned callee parameter permits normal retirement.
/// Reverse publication preserves the existing first-to-last user-argument cleanup order.
pub(super) fn root_user_call_operands(
    ctx: &mut LoweringContext<'_, '_>,
    operands: &mut [crate::ir::ValueId],
    signature: Option<&FunctionSig>,
    return_alias: &ReturnArgAlias,
    result_type: &PhpType,
    span: Span,
) -> Vec<(usize, crate::ir::LocalSlotId)> {
    if signature.is_some_and(|sig| sig.by_ref_return) {
        return Vec::new();
    }
    let independent_result = !Ownership::php_type_needs_lifetime_tracking(result_type)
        || return_alias == &ReturnArgAlias::None;
    let mut roots = Vec::new();
    for (index, operand) in operands.iter_mut().enumerate().rev() {
        if signature.is_some_and(|sig| sig.ref_params.get(index).copied().unwrap_or(false))
            && !matches!(ctx.builder.value_defining_op(*operand), Some(Op::ArrayNew | Op::HashNew))
        {
            // Actual reference places must preserve their caller storage. A fresh default
            // container has no place: backend staging retains its own managed-cell payload,
            // and the original EIR owner must also retire when the callee throws.
            continue;
        }
        if !independent_result
            && !signature.is_some_and(|sig| sig.returned_parameter_has_independent_owner(index))
        {
            continue;
        }
        let value = LoweredValue { value: *operand, ir_type: ctx.builder.value_type(*operand) };
        let (value, root) = root_owned_call_operand(ctx, value, span);
        *operand = value.value;
        if let Some(slot) = root { roots.push((index, slot)); }
    }
    roots
}

/// Publishes a temporary owner in a frame slot and returns its stable invocation operand.
pub(crate) fn root_owned_call_operand(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<crate::ir::LocalSlotId>) {
    if !ctx.value_needs_release_after_use(value) {
        return (value, None);
    }
    let ty = ctx.builder.value_php_type(value.value);
    if matches!(ty.codegen_repr(), PhpType::Buffer(_)) { return (value, None); }
    let name = ctx.declare_hidden_temp(ty.clone());
    // Concrete local loads can still be provisional owned unboxes until frame
    // types are finalized. Acquire explicitly, then let ownership finalization
    // prune the source release when that load turns out to be borrowed.
    let rooted = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.store_local(&name, rooted, ty, Some(span));
    let slot = ctx.local_slots[&name];
    register_owned_call_operand(ctx, slot, span);
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    (rooted, Some(slot))
}

/// Makes a rooted operand visible even to a catch that preserves the current PHP activation.
pub(super) fn register_owned_call_operand(
    ctx: &mut LoweringContext<'_, '_>,
    slot: crate::ir::LocalSlotId,
    span: Span,
) {
    ctx.emit_void(
        Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PushCallOperandOwner.default_effects(), Some(span),
    );
}

/// Removes a published owner record without releasing the slot it protects.
fn unregister_owned_call_operand(
    ctx: &mut LoweringContext<'_, '_>,
    slot: crate::ir::LocalSlotId,
    span: Span,
) {
    ctx.emit_void(
        Op::PopCallOperandOwner,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        Op::PopCallOperandOwner.default_effects(),
        Some(span),
    );
}

/// Clears a rooted operand before releasing it, including when its destructor throws.
pub(crate) fn retire_owned_call_operand(
    ctx: &mut LoweringContext<'_, '_>,
    slot: crate::ir::LocalSlotId,
    span: Span,
) {
    unregister_owned_call_operand(ctx, slot, span);
    ctx.emit_void(
        Op::ReleaseLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(),
        Some(span),
    );
}
