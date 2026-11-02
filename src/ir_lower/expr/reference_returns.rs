//! Purpose:
//! Consumes the owned cell transferred by a reference-returning PHP call.
//!
//! Called from:
//! - Direct function, method and statically resolved callable lowering.
//!
//! Key details:
//! - A reference assignment adopts the lease into staging it published BEFORE the call.
//! - An ordinary value use publishes its own staging before the arguments and copies the
//!   payload out only once the caller's own cleanup can no longer throw.
//! - That copy DETACHES a `Mixed` pointee instead of retaining the callee's mutable box, so a
//!   later write through the same reference cannot be observed by the by-value result.
//! - A replaced binding stays rooted until its new alias is visible, including loop back-edges.

use super::*;

/// Staging an ordinary by-value use of a reference-returning call publishes before its arguments.
///
/// The lease the callee transfers is the only thing keeping the pointee alive, so it stays in
/// this slot while the caller retires argument temporaries, evaluation intermediates and an
/// owning receiver. Those steps run PHP destructors and can throw, and the published record is
/// what releases the lease when they do.
pub(super) struct ReferenceReturnValueStaging {
    /// Hidden local the transferred cell is adopted into.
    staged: String,
    /// Owner slot published in the unwind chain before the source arguments were lowered.
    owner: crate::ir::LocalSlotId,
}

/// A previous raw payload or nullable cell lease held through reference rebinding.
pub(super) enum PreviousReferenceBindingOwner {
    /// A definitely initialized local lends a payload that can be retained directly.
    Payload(crate::ir::LocalSlotId),
    /// A loop back-edge can own a cell even at the first syntactic binding.
    Cell { staged: String, owner: crate::ir::LocalSlotId },
}

/// Defers an old target's last payload release until its replacement alias is published.
pub(super) fn root_previous_reference_binding(
    ctx: &mut LoweringContext<'_, '_>, target: &str, span: Span,
) -> Option<PreviousReferenceBindingOwner> {
    let initialized = ctx.local_slots.get(target).copied()
        .is_some_and(|slot| ctx.slot_is_initialized(slot));
    if !initialized {
        // Owner slots start at zero, but the same syntactic assignment can
        // encounter a managed cell on its next execution through a loop.
        let source = ctx.declare_ref_cell_owner(target, ctx.local_type(target));
        let (staged, owner) = ctx.predeclare_returned_ref_cell_staging();
        ctx.emit_void(Op::RetainLocalRefCell, Vec::new(),
            Some(Immediate::LocalSlotPair { first: source, second: owner }),
            Op::RetainLocalRefCell.default_effects(), Some(span));
        register_owned_call_operand(ctx, owner, span);
        return Some(PreviousReferenceBindingOwner::Cell { staged, owner });
    }
    if !Ownership::php_type_needs_lifetime_tracking(&ctx.local_type(target)) { return None; }
    let previous = ctx.load_local(target, Some(span));
    let retained = crate::ir_lower::ownership::acquire_if_refcounted(ctx, previous, Some(span));
    // This record nests inside the returned lease, so a throwing old destructor
    // retires the old payload first while the replacement remains staged and aliased.
    let owner = root_owned_call_operand(ctx, retained, span).1;
    // Later rebinding can widen the raw slot to Mixed. Its earlier concrete
    // load then owns an unboxed payload independently of the explicit retain.
    crate::ir_lower::ownership::release_if_owned(ctx, previous, Some(span));
    owner.map(PreviousReferenceBindingOwner::Payload)
}

/// Retires the previous binding only after the target already names its replacement.
pub(super) fn retire_previous_reference_binding(
    ctx: &mut LoweringContext<'_, '_>, owner: PreviousReferenceBindingOwner, span: Span,
) {
    match owner {
        PreviousReferenceBindingOwner::Payload(owner) => retire_owned_call_operand(ctx, owner, span),
        PreviousReferenceBindingOwner::Cell { staged, owner } => {
            unregister_owned_call_operand(ctx, owner, span);
            ctx.release_ref_cell_owner(&staged, Some(span));
        }
    }
}

/// Publishes the lease slot an ordinary by-value reference return will adopt into.
///
/// Called before the call's argument evaluation so the record nests OUTSIDE every root the
/// arguments publish, which is what lets the lease outlive their retirement. The owner slot is
/// zero-initialized in the frame prologue, so publishing it before anything is adopted releases
/// nothing. Returns `None` for a callee that does not transfer a cell, and for the one whose
/// result an enclosing reference assignment already staged.
pub(super) fn begin_reference_return_call(
    ctx: &mut LoweringContext<'_, '_>,
    signature: Option<&FunctionSig>,
    span: Span,
) -> Option<ReferenceReturnValueStaging> {
    if !signature.is_some_and(|signature| signature.by_ref_return) {
        return None;
    }
    if ctx
        .reference_call_context
        .as_ref()
        .is_some_and(|context| context.depth == ctx.expression_depth)
    {
        return None;
    }
    let (staged, owner) = ctx.predeclare_returned_ref_cell_staging();
    register_owned_call_operand(ctx, owner, span);
    Some(ReferenceReturnValueStaging { staged, owner })
}

/// Adopts the cell a reference-returning call transferred, before any caller cleanup runs.
///
/// Both outcomes adopt into a hidden owner slot here, which is immediately after the call and
/// before argument temporaries, evaluation intermediates or an owning receiver are retired.
///
/// - A reference assignment still has to retire the previous binding of its target and publish
///   the alias, and either step can run a destructor that throws. Its staging was therefore
///   declared and published in the unwind chain by `lower_ref_assign_call` before the source
///   expression was lowered, so this adoption drops the lease straight into a record that is
///   already live and nests OUTSIDE every argument root the call published.
/// - An ordinary value use adopts into the staging `begin_reference_return_call` published,
///   and keeps the payload alive through that lease alone until
///   `finish_reference_return_value` copies it out.
///
/// The returned value is the raw cell the call produced, which is what the caller's argument
/// cleanup inspects for aliasing. A by-reference-returning callee owns its result cell
/// independently of every by-value argument, so that cleanup releases those arguments either
/// way and never compares the cell against them.
pub(super) fn finish_reference_return_call(
    ctx: &mut LoweringContext<'_, '_>,
    call: LoweredValue,
    signature: Option<&FunctionSig>,
    staging: Option<&ReferenceReturnValueStaging>,
    span: Span,
) -> LoweredValue {
    if !signature.is_some_and(|signature| signature.by_ref_return) {
        return call;
    }
    let php_type = ctx.builder.value_php_type(call.value);
    let assignment_staging = ctx
        .reference_call_context
        .as_ref()
        .filter(|context| context.depth == ctx.expression_depth)
        .map(|context| context.staged.clone());
    if let Some(staged) = assignment_staging {
        ctx.adopt_returned_ref_cell_into(&staged, call, php_type, Some(span));
        if let Some(context) = ctx.reference_call_context.as_mut() {
            context.adopted = true;
        }
        return call;
    }
    let Some(staging) = staging else {
        // Every call site resolves its signature BEFORE evaluating arguments and publishes its
        // staging there, and `begin_reference_return_call` reads the same signature and the same
        // reference-assignment context this function does, so exactly one of the two branches
        // above always applies. Consuming the lease here instead would reintroduce the unrooted
        // copy gap the staging exists to close, so this fails closed rather than degrading.
        unreachable!(
            "a by-reference-returning call publishes its lease staging before its arguments"
        )
    };
    ctx.adopt_returned_ref_cell_into(&staging.staged, call, php_type, Some(span));
    call
}

/// Copies the referenced payload out of the lease once caller cleanup can no longer throw.
///
/// The copy is acquired before the lease is retired, and the record is detached before the
/// release, exactly as `retire_owned_call_operand` does for a value root: a throwing payload
/// destructor must not be retried by a later walk of the cleanup chain. Retiring here, after
/// the argument roots and intermediates this call published, keeps the chain strictly LIFO.
pub(super) fn finish_reference_return_value(
    ctx: &mut LoweringContext<'_, '_>,
    call: LoweredValue,
    staging: Option<ReferenceReturnValueStaging>,
    span: Span,
) -> LoweredValue {
    let Some(staging) = staging else {
        return call;
    };
    let value = ctx.load_local(&staging.staged, Some(span));
    let owned = detach_reference_return_payload(ctx, value, span);
    unregister_owned_call_operand(ctx, staging.owner, span);
    ctx.release_ref_cell_owner(&staging.staged, Some(span));
    owned
}

/// Makes the copied payload independent of the cell the callee transferred.
///
/// A `Mixed` payload is DETACHED with `MixedClone` rather than merely retained: the lease keeps
/// the callee's mutable box, and a later write through that same reference, such as an append to
/// the static property the cell addresses, mutates the box in place. A by-value use of the call
/// must not see that write, so it gets its own box. This is the identical rule
/// `acquire_borrowed_return_value` applies to a by-value `return` of a `LoadRefCell`, and the
/// runtime clone preserves the resource special case.
///
/// Every other refcounted payload stays on the plain acquire. An array is copy-on-write, so the
/// extra reference is exactly PHP's by-value array copy and a later write through the reference
/// splits the payload first; retaining also keeps the existing alias behaviour a reference
/// writeback into the same cell relies on.
fn detach_reference_return_payload(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    let php_type = ctx.builder.value_php_type(value.value);
    if ctx.builder.value_defining_op(value.value) == Some(Op::LoadRefCell)
        && php_type.codegen_repr() == PhpType::Mixed
    {
        return ctx.emit_owned_value(
            Op::MixedClone,
            vec![value.value],
            None,
            php_type,
            Op::MixedClone.default_effects(),
            Some(span),
        );
    }
    crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
}
