//! Purpose:
//! Roots owned expression operands across calls that can unwind out of their caller.
//!
//! Called from:
//! - User calls, descriptor invocation, callback builtins and eval call lowering.
//! - Any lowering that emits an instruction whose effects carry `MAY_THROW` over owned
//!   expression temporaries, through `pin_in_flight_owners`.
//!
//! Key details:
//! - Scoped unwind records retire roots before same-frame catches; frame cleanup sees cleared slots.
//! - Borrowed local loads remain subject to final storage-aware release pruning.
//! - A call's own owned result is staged in a record published OUTSIDE every operand root, so
//!   retiring those roots cannot strand it; the runtime pop is LIFO and never names a record.
//! - Rooting MOVES ownership into a slot; pinning does not. A pin only parks the pointer for
//!   the window one throwing instruction occupies, so the operand's existing release still runs
//!   on the normal path and only the unwinding path releases through the record.

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

/// Publishes a managed synthetic receiver alias while one call argument is evaluated.
///
/// Nested property and element receivers are normalized through local reference aliases. Those
/// aliases are expression temporaries, not frame-lifetime PHP variables, so their cell owners
/// must join the call's unwind ledger and retire as soon as the final element cell is leased.
pub(super) fn publish_scoped_ref_receiver_alias(
    ctx: &mut LoweringContext<'_, '_>,
    alias: &str,
    span: Span,
) -> bool {
    let Some(slot) = ctx.ref_cell_owner_slot(alias) else {
        return false;
    };
    if ctx.call_argument_evaluation_scopes.is_empty() {
        return false;
    }
    register_owned_call_operand(ctx, slot, span);
    ctx.call_argument_evaluation_scopes
        .last_mut()
        .expect("call argument evaluation scope")
        .owners
        .push(crate::ir_lower::context::CallArgumentEvaluationOwner {
            value: None,
            borrow: None,
            temp_name: alias.to_string(),
            slot,
            span,
            ref_cell_payload: Some(PhpType::Mixed),
            scoped_receiver_alias: true,
        });
    true
}

/// Retires nested receiver aliases after the final managed element cell has been leased.
///
/// The final lease remains the top unwind record while releasing aliases, since those releases
/// can run destructors and throw. Once every alias slot is cleared, the non-throwing record edits
/// temporarily detach that lease, discard the now-empty alias records in LIFO order, and publish
/// the final lease again before later argument evaluation begins.
pub(super) fn retire_scoped_ref_receiver_aliases(
    ctx: &mut LoweringContext<'_, '_>,
    aliases: &[String],
) {
    if aliases.is_empty() {
        return;
    }
    let Some(scope) = ctx.call_argument_evaluation_scopes.last() else {
        return;
    };
    let Some(final_alias) = aliases.last() else {
        return;
    };
    let Some(final_owner) = scope
        .owners
        .iter()
        .rev()
        .find(|owner| owner.scoped_receiver_alias && owner.temp_name == *final_alias)
        .cloned()
    else {
        return;
    };
    let intermediate_aliases = &aliases[..aliases.len() - 1];
    let alias_owners: Vec<_> = intermediate_aliases
        .iter()
        .filter_map(|alias| {
            scope
                .owners
                .iter()
                .rev()
                .find(|owner| owner.scoped_receiver_alias && owner.temp_name == *alias)
                .cloned()
        })
        .collect();
    if alias_owners.len() != intermediate_aliases.len() {
        return;
    }

    for owner in alias_owners.iter().rev() {
        ctx.release_ref_cell_owner(&owner.temp_name, Some(owner.span));
    }
    unregister_owned_call_operand(ctx, final_owner.slot, final_owner.span);
    for owner in alias_owners.iter().rev() {
        unregister_owned_call_operand(ctx, owner.slot, owner.span);
    }
    register_owned_call_operand(ctx, final_owner.slot, final_owner.span);

    let alias_slots: std::collections::HashSet<_> =
        alias_owners.iter().map(|owner| owner.slot).collect();
    ctx.call_argument_evaluation_scopes
        .last_mut()
        .expect("call argument evaluation scope")
        .owners
        .retain(|owner| !alias_slots.contains(&owner.slot));
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
    let php_type = ctx.builder.value_php_type(value.value);
    let borrowed_ref_cell_value = ctx.builder.value_defining_op(value.value) == Some(Op::LoadRefCell)
        && Ownership::php_type_needs_lifetime_tracking(&php_type);
    if !ctx
        .call_argument_evaluation_scopes
        .last()
        .is_some_and(|scope| scope.expression_depth == ctx.expression_depth)
        || (!ctx.value_needs_release_after_use(value) && !borrowed_ref_cell_value)
    {
        return value;
    }
    let ty = php_type;
    if matches!(ty.codegen_repr(), PhpType::Buffer(_)) {
        return value;
    }
    let temp_name = ctx.declare_owned_hidden_temp(ty.clone());
    let rooted =
        crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, value, Some(span));
    ctx.store_local(&temp_name, rooted, ty, Some(span));
    let slot = ctx.local_slots[&temp_name];
    register_owned_call_operand(ctx, slot, span);
    if !borrowed_ref_cell_value {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
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
            value: Some(rooted.value),
            borrow: Some(borrowed),
            temp_name,
            slot,
            span,
            ref_cell_payload: None,
            scoped_receiver_alias: false,
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
) -> Vec<CallArgumentIntermediate> {
    let scope = ctx
        .call_argument_evaluation_scopes
        .pop()
        .expect("call argument evaluation scope must be balanced");
    debug_assert_eq!(scope.expression_depth, ctx.expression_depth);
    let mut intermediates = Vec::new();
    for owner in scope.owners.into_iter().rev() {
        unregister_owned_call_operand(ctx, owner.slot, owner.span);
        if let Some(payload_type) = owner.ref_cell_payload {
            // Keep the Borrow marker on the call operand. It tells codegen that the
            // surrounding EIR ledger, rather than the ABI materializer, retires this
            // already-acquired managed cell after the call.
            intermediates.push(CallArgumentIntermediate {
                slot: owner.slot,
                span: owner.span,
                ref_cell_payload: Some(payload_type),
            });
            continue;
        }
        let owner_value = owner
            .value
            .expect("ordinary call owner has a rooted value");
        let owner_borrow = owner
            .borrow
            .expect("ordinary call owner has a borrowed value");
        let mut retained_by_call = false;
        for operand in operands.iter_mut() {
            if *operand == owner_borrow {
                *operand = owner_value;
                retained_by_call = true;
            } else if *operand == owner_value {
                retained_by_call = true;
            }
        }
        if retained_by_call {
            ctx.clear_owned_hidden_temp(&owner.temp_name, Some(owner.span));
        } else {
            intermediates.push(CallArgumentIntermediate {
                slot: owner.slot,
                span: owner.span,
                ref_cell_payload: None,
            });
        }
    }
    intermediates.reverse();
    for intermediate in &intermediates {
        register_owned_call_operand(ctx, intermediate.slot, intermediate.span);
    }
    intermediates
}

/// Owner retained while argument evaluation and the enclosing call are in flight.
pub(super) struct CallArgumentIntermediate {
    slot: crate::ir::LocalSlotId,
    span: Span,
    ref_cell_payload: Option<PhpType>,
}

/// Retires source-evaluation leases after the enclosing call's ordinary cleanup.
///
/// Every record is detached before its owner is released. `ReleaseLocalRefCell` clears the owner
/// slot first and completes the cell and payload retirement inside a bounded cleanup boundary
/// before rethrowing, so its record no longer owns useful recovery state. Detaching it exposes
/// the older argument leases and prepublished call result to the outer unwinder.
pub(super) fn retire_call_argument_intermediates(
    ctx: &mut LoweringContext<'_, '_>,
    roots: &[CallArgumentIntermediate],
) {
    for root in roots.iter().rev() {
        unregister_owned_call_operand(ctx, root.slot, root.span);
        if let Some(payload_type) = &root.ref_cell_payload {
            ctx.builder.emit_with_effects(
                Op::ReleaseLocalRefCell,
                Vec::new(),
                Some(Immediate::LocalSlot(root.slot)),
                crate::ir::IrType::Void,
                payload_type.clone(),
                Ownership::NonHeap,
                Op::ReleaseLocalRefCell.default_effects(),
                Some(root.span),
            );
        } else {
            ctx.emit_void(
                Op::ReleaseLocalSlot,
                Vec::new(),
                Some(Immediate::LocalSlot(root.slot)),
                Op::ReleaseLocalSlot.default_effects(),
                Some(root.span),
            );
        }
    }
}

/// Retains a managed element cell before a later argument can replace its parent container.
///
/// Calls that use the argument-evaluation ledger retire the lease immediately after their call
/// and ordinary writebacks. Legacy call surfaces receive the bare `AcquireRefCell` result, which
/// lets their shared ABI materializer identify and retire the already-published lease. Acquiring
/// here is essential because later argument evaluation can replace the parent container before
/// backend call materialization begins.
pub(super) fn lease_managed_call_argument_ref_cell(
    ctx: &mut LoweringContext<'_, '_>,
    cell_ptr: LoweredValue,
    span: Span,
) -> LoweredValue {
    // Ref-place preparation may lower synthetic receiver expressions below the depth at which
    // the surrounding call opened its source-order ledger. The innermost active scope is still
    // authoritative: a nested call pushes and retires its own scope before returning here.
    let has_evaluation_scope = !ctx.call_argument_evaluation_scopes.is_empty();
    let (temp_name, owner) = ctx.predeclare_returned_ref_cell_staging();
    register_owned_call_operand(ctx, owner, span);
    let captured = ctx.emit_value(
        Op::AcquireRefCell,
        vec![cell_ptr.value],
        Some(Immediate::LocalSlot(owner)),
        PhpType::Pointer(None),
        Op::AcquireRefCell.default_effects(),
        Some(span),
    );
    if !has_evaluation_scope {
        return captured;
    }
    let borrowed = ctx
        .builder
        .emit_with_effects(
            Op::Borrow,
            vec![captured.value],
            None,
            captured.ir_type,
            PhpType::Pointer(None),
            Ownership::NonHeap,
            Op::Borrow.default_effects(),
            Some(span),
        )
        .expect("managed call argument borrow produces a value");
    ctx.call_argument_evaluation_scopes
        .last_mut()
        .expect("call argument evaluation scope")
        .owners
        .push(crate::ir_lower::context::CallArgumentEvaluationOwner {
            value: Some(captured.value),
            borrow: Some(borrowed),
            temp_name,
            slot: owner,
            span,
            ref_cell_payload: Some(PhpType::Mixed),
            scoped_receiver_alias: false,
        });
    LoweredValue {
        value: borrowed,
        ir_type: captured.ir_type,
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

/// One in-flight owned temporary pinned in an unwind-visible frame slot for a single
/// throwing instruction.
///
/// The pin does NOT move ownership. The slot receives a plain `OwnedTemp` store, which retains
/// nothing, so the SSA temporary keeps its single reference and its ordinary post-instruction
/// release stays correct on the normal path.
pub(crate) struct PinnedInFlightOwner {
    /// Hidden `OwnedTemp` the operand pointer is parked in for the throwing window.
    temp_name: String,
    /// Owner slot published in the unwind chain for the duration of that window.
    slot: crate::ir::LocalSlotId,
}

/// Pins the owned SSA temporaries an instruction that can THROW is about to read.
///
/// A codegen guard that raises a catchable `Error`/`TypeError` jumps straight to
/// `__rt_throw_current`, which only releases what the activation chain can see. An expression
/// temporary is pure SSA at that point: `count(pick(new Other()))` holds the boxed `Mixed` the
/// inner call produced in nothing the unwinder can reach, so a caught `TypeError` used to resume
/// with that box and its object stranded (issue: repeated caught guards leak one operand per
/// iteration).
///
/// Pinning publishes a record over the throwing window only. On the throwing path
/// `__rt_cleanup_call_operand_owner` CLEARS the slot before releasing it, so the frame cleanup
/// that follows cannot release the same owner twice, and the never-reached SSA release cannot
/// either. On the normal path the record is detached and the slot is zeroed with `UnsetLocal`,
/// which retires the pin WITHOUT releasing, leaving the operand's existing ownership discipline
/// exactly as it was.
///
/// Only a temporary that OWNS independent storage is pinned. A persistent literal, a borrowed
/// projection and every load out of a variable are left alone, because a pin retains nothing and
/// would otherwise publish a reference some other owner is still responsible for. Repeated
/// operands (`f($x, $x)`) are pinned once: two records over one reference would release it twice.
pub(crate) fn pin_in_flight_owners(
    ctx: &mut LoweringContext<'_, '_>,
    operands: &[crate::ir::ValueId],
    span: Span,
) -> Vec<PinnedInFlightOwner> {
    let mut pinned: Vec<crate::ir::ValueId> = Vec::new();
    let mut records = Vec::new();
    for operand in operands {
        if pinned.contains(operand) {
            continue;
        }
        let value = LoweredValue {
            value: *operand,
            ir_type: ctx.builder.value_type(*operand),
        };
        // The test is ownership of INDEPENDENT storage, not "this path emits a release".
        // `value_needs_release_after_use` also answers yes for a plain `Str` load out of a PHP
        // local, whose payload the local slot still owns: parking that pointer would let the
        // record free storage the frame cleanup frees again (`explode("", $local)` double-freed
        // exactly that way). Rooting can use the wider test because it acquires first; a pin
        // acquires nothing, so it must see a temporary that owns what it parks.
        if !ctx.value_is_owning_temporary(value) {
            continue;
        }
        // A load out of a variable is never such a temporary, whatever the provisional
        // ownership says. `value_is_owned_unboxed_local_load` marks an array/hash/object load
        // owned so a release can be emitted and pruned later, and the variable keeps owning the
        // payload either way: publishing it would run the element destructors of a LIVE `$values`
        // during the unwind, before the catch body that PHP runs first
        // (`implode(",", $values)` with a throwing `__toString` reordered exactly that way).
        if matches!(
            ctx.builder.value_defining_op(value.value),
            Some(Op::LoadLocal | Op::LoadStaticLocal | Op::LoadRefCell | Op::LoadGlobal),
        ) {
            continue;
        }
        let ty = ctx.builder.value_php_type(value.value);
        // A raw buffer carries no refcount, and a type with no lifetime state has nothing an
        // unwind could strand.
        if matches!(ty.codegen_repr(), PhpType::Buffer(_))
            || !Ownership::php_type_needs_lifetime_tracking(&ty)
        {
            continue;
        }
        let temp_name = ctx.declare_owned_hidden_temp(ty.clone());
        ctx.store_local(&temp_name, value, ty, Some(span));
        let slot = ctx.local_slots[&temp_name];
        register_owned_call_operand(ctx, slot, span);
        pinned.push(*operand);
        records.push(PinnedInFlightOwner { temp_name, slot });
    }
    records
}

/// Retires in-flight pins in reverse publication order once the throwing window has closed.
///
/// `UnsetLocal` zeroes the parking slot without releasing it: the reference never left the SSA
/// temporary, and its own release still follows.
pub(crate) fn unpin_in_flight_owners(
    ctx: &mut LoweringContext<'_, '_>,
    records: Vec<PinnedInFlightOwner>,
    span: Span,
) {
    for record in records.into_iter().rev() {
        unregister_owned_call_operand(ctx, record.slot, span);
        ctx.clear_owned_hidden_temp(&record.temp_name, Some(span));
    }
}

/// Pins the owned operands a throwing registry builtin is about to read.
///
/// The rule is the builtin's own effect contract, not its name: any builtin whose resolved
/// effects carry `MAY_THROW` can reach a codegen guard that jumps to `__rt_throw_current` with
/// its operands still in flight, so every one of them gets the same treatment. `count()` is the
/// case the leak was found on; `intdiv()`, the `ValueError` argument guards and the weak
/// float-to-int coercions reach the same helper through the same path.
///
/// Operands already rooted by `root_non_aliasing_callback_operands` are skipped: they carry a
/// record of their own, and a second record over one reference would release it twice.
pub(super) fn pin_throwing_builtin_operands(
    ctx: &mut LoweringContext<'_, '_>,
    def: &crate::builtins::registry::BuiltinDef,
    operands: &[crate::ir::ValueId],
    rooted: &[(usize, crate::ir::LocalSlotId)],
    result_type: &PhpType,
    span: Span,
) -> Vec<PinnedInFlightOwner> {
    let arg_types = operands
        .iter()
        .map(|operand| ctx.builder.value_php_type(*operand))
        .collect::<Vec<_>>();
    let input = crate::builtins::semantics::BuiltinSemanticInput {
        name: def.name,
        args: &[],
        arg_types: &arg_types,
        span,
    };
    if !crate::builtins::semantics::resolve_builtin_effects(def, &input)
        .contains(crate::ir::Effects::MAY_THROW)
    {
        return Vec::new();
    }
    // A result that may alias an argument suppresses that argument's post-call release, so the
    // call, not this path, decides the operand's fate. Pinning it would add a reference nothing
    // retires. Only an independently owned result leaves the caller still owing the release a
    // pin stands in for while the call can throw.
    //
    // A result whose storage carries no lifetime state at all settles the same question from the
    // other side: `count()` sits in the default `MayAliasArguments` bucket, but it answers a raw
    // machine integer, which `release_owned_call_arg_temporaries` already knows cannot alias its
    // boxed operand, so that operand's release is emitted and the pin stands in for it.
    let independent_result = matches!(
        def.spec.semantics.result_ownership,
        crate::builtins::semantics::BuiltinResultOwnership::NonHeap
            | crate::builtins::semantics::BuiltinResultOwnership::Fresh
            | crate::builtins::semantics::BuiltinResultOwnership::Independent
    ) || !Ownership::php_type_needs_lifetime_tracking(result_type);
    if !independent_result {
        return Vec::new();
    }
    let pinnable = operands
        .iter()
        .enumerate()
        .filter(|(index, _)| !rooted.iter().any(|(root, _)| root == index))
        // A mutating by-reference parameter is caller storage the builtin writes back through,
        // never a temporary this path releases.
        .filter(|(index, _)| !def.ref_params.get(*index).copied().unwrap_or(false))
        .map(|(_, operand)| *operand)
        .collect::<Vec<_>>();
    pin_in_flight_owners(ctx, &pinnable, span)
}
