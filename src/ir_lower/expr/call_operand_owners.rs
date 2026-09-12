//! Purpose:
//! Roots owned expression operands across calls that can unwind out of their caller.
//!
//! Called from:
//! - User calls, descriptor invocation, callback builtins and eval call lowering.
//!
//! Key details:
//! - Scoped unwind records retire roots before same-frame catches; frame cleanup sees cleared slots.
//! - Borrowed local loads remain subject to final storage-aware release pruning.
//! - A call's own owned result is staged in a record published OUTSIDE every operand root, so
//!   retiring those roots cannot strand it; the runtime pop is LIFO and never names a record.

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
///
/// A by-reference-returning callee participates on exactly the same terms. Its result is a
/// reference cell, never an unretained alias of a by-value argument payload, which is the same
/// fact `release_owned_call_arg_temporaries_with_roots` already relies on when it releases those
/// arguments after the call. Rooting them only makes that release reachable from a throwing
/// callee as well; the payload the returned cell addresses is retained by the cell itself, so a
/// fresh omitted-reference default is rooted here for every callee kind rather than being left
/// unreachable from cleanup on the one that returns a reference.
pub(super) fn root_user_call_operands(
    ctx: &mut LoweringContext<'_, '_>,
    operands: &mut [crate::ir::ValueId],
    signature: Option<&FunctionSig>,
    return_alias: &ReturnArgAlias,
    result_type: &PhpType,
    span: Span,
) -> Vec<(usize, crate::ir::LocalSlotId)> {
    let independent_result = signature.is_some_and(|sig| sig.by_ref_return)
        || !Ownership::php_type_needs_lifetime_tracking(result_type)
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

/// A frame slot published BEFORE a call's operand roots so the call's own owned result stays
/// reachable from exception cleanup while those roots retire.
///
/// Retiring an argument root, an evaluation intermediate, a descriptor callback, an argument
/// container or an owning receiver runs PHP destructors, and any of them can throw into a catch
/// in this same PHP frame. The result the call already produced is only an SSA temporary at that
/// point, so without this staging nothing the unwind chain can see owns it.
///
/// The record is published before every operand root and popped after all of them, which is what
/// keeps the chain strictly LIFO: the runtime pop detaches the innermost record and never looks
/// for a named one.
pub(super) struct PrepublishedCallResult {
    /// Hidden one-shot temporary the owned result is moved into.
    temp_name: String,
    /// Owner slot published in the unwind chain before any operand root.
    slot: crate::ir::LocalSlotId,
    /// Storage type the slot was declared with, which the call must produce exactly.
    php_type: PhpType,
}

/// Publishes the staging slot a call's owned result is moved into.
///
/// Called before the call's operand roots are published, and therefore before the result type can
/// be read back from the call, so the caller passes the exact `PhpType` it will emit the call
/// with. Returns `None` for a result whose storage carries no runtime lifetime state, and for a
/// `Buffer`, whose raw storage is not refcounted.
///
/// The slot is an `OwnedTemp`, which the frame prologue zero-initializes and whose store MOVES
/// its source instead of retaining it, so publishing it before anything is stored releases
/// nothing and staging the result never doubles its reference.
pub(super) fn prepublish_call_result(
    ctx: &mut LoweringContext<'_, '_>,
    result_type: &PhpType,
    span: Span,
) -> Option<PrepublishedCallResult> {
    if !Ownership::php_type_needs_lifetime_tracking(result_type)
        || matches!(result_type.codegen_repr(), PhpType::Buffer(_))
    {
        return None;
    }
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let slot = ctx.local_slots[&temp_name];
    register_owned_call_operand(ctx, slot, span);
    Some(PrepublishedCallResult {
        temp_name,
        slot,
        php_type: result_type.clone(),
    })
}

/// Publishes result staging for a direct user call whose result owns storage independently.
///
/// A by-reference-returning callee is excluded because its result is the transferred cell, which
/// `begin_reference_return_call` stages instead. A callee summarized as possibly returning one of
/// its arguments is excluded too: the caller's argument release is guarded by a runtime alias
/// comparison, so rooting both the argument slot and the result slot would release one shared
/// reference twice while unwinding.
pub(super) fn prepublish_user_call_result(
    ctx: &mut LoweringContext<'_, '_>,
    signature: Option<&FunctionSig>,
    return_alias: &ReturnArgAlias,
    result_type: &PhpType,
    span: Span,
) -> Option<PrepublishedCallResult> {
    if signature.is_some_and(|signature| signature.by_ref_return) {
        return None;
    }
    if return_alias != &ReturnArgAlias::None {
        return None;
    }
    prepublish_call_result(ctx, result_type, span)
}

/// Moves the owned call result into its published staging slot, immediately after the call.
///
/// The store is a plain `OwnedTemp` move: no acquire, no coercion, so the single reference the
/// call handed back now lives in a slot the unwind chain can release. The declared slot type and
/// the call's own result type are the same value at every call site; a mismatch would silently
/// release the wrong storage shape, so it fails closed instead of degrading to no rooting.
pub(super) fn stage_call_result(
    ctx: &mut LoweringContext<'_, '_>,
    staging: Option<&PrepublishedCallResult>,
    result: LoweredValue,
    span: Span,
) {
    let Some(staging) = staging else {
        return;
    };
    let produced = ctx.builder.value_php_type(result.value);
    assert_eq!(
        produced.codegen_repr(),
        staging.php_type.codegen_repr(),
        "a prepublished call result slot is declared with the call's own result type",
    );
    ctx.store_local(&staging.temp_name, result, staging.php_type.clone(), Some(span));
}

/// Retires result staging once no further caller cleanup can throw, keeping the result owned.
///
/// The record is detached first and the slot is then zeroed with `UnsetLocal`, which transfers
/// the reference back to the SSA result without releasing it. Using `ReleaseLocalSlot` here would
/// free the value the expression is about to hand to its consumer.
pub(super) fn take_prepublished_call_result(
    ctx: &mut LoweringContext<'_, '_>,
    staging: Option<PrepublishedCallResult>,
    result: LoweredValue,
    span: Span,
) -> LoweredValue {
    let Some(staging) = staging else {
        return result;
    };
    unregister_owned_call_operand(ctx, staging.slot, span);
    ctx.clear_owned_hidden_temp(&staging.temp_name, Some(span));
    result
}

/// Publishes a freshly created container before the expressions that fill it can throw.
///
/// A container built element by element is the only owner of everything already inserted, so it
/// has to be reachable from exception cleanup for the whole construction, not just at the call
/// that finally consumes it.
pub(super) fn publish_constructed_container(
    ctx: &mut LoweringContext<'_, '_>,
    container: LoweredValue,
    span: Span,
) -> crate::ir::LocalSlotId {
    let (_, owner) = root_owned_call_operand(ctx, container, span);
    owner.expect("a fresh container must have a managed owner")
}

/// Borrows the published container through its slot so growth writes the new pointer back.
pub(super) fn load_published_container(
    ctx: &mut LoweringContext<'_, '_>,
    slot: crate::ir::LocalSlotId,
    php_type: PhpType,
    span: Span,
) -> LoweredValue {
    let value = ctx.emit_value(
        Op::LoadLocal,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        php_type,
        Op::LoadLocal.default_effects(),
        Some(span),
    );
    ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
    value
}

/// Transfers a completed container out of its published construction slot.
pub(super) fn take_published_container(
    ctx: &mut LoweringContext<'_, '_>,
    slot: crate::ir::LocalSlotId,
    php_type: PhpType,
    span: Span,
) -> LoweredValue {
    let borrowed = load_published_container(ctx, slot, php_type, span);
    let owned = crate::ir_lower::ownership::acquire_if_refcounted(ctx, borrowed, Some(span));
    retire_owned_call_operand(ctx, slot, span);
    owned
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
pub(crate) fn unregister_owned_call_operand(
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
