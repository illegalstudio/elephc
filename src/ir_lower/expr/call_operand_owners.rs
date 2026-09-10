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

/// Roots non-reference callback-builtin operands when the result cannot alias their ownership.
pub(super) fn root_non_aliasing_callback_operands(
    ctx: &mut LoweringContext<'_, '_>,
    def: &crate::builtins::registry::BuiltinDef,
    operands: &mut [crate::ir::ValueId],
    result_type: &PhpType,
    span: Span,
) -> Vec<(usize, crate::ir::LocalSlotId)> {
    use crate::builtins::semantics::{BuiltinLowering, BuiltinResultOwnership};
    use crate::ir::RuntimeCallTarget;
    let BuiltinLowering::Runtime(RuntimeCallTarget::Function(target)
        | RuntimeCallTarget::ProfiledFunction { target, .. }) = def.spec.semantics.lowering else {
        return Vec::new();
    };
    let independent_result = !crate::ir::Ownership::php_type_needs_lifetime_tracking(result_type)
        || matches!(
            def.spec.semantics.result_ownership,
            BuiltinResultOwnership::NonHeap | BuiltinResultOwnership::Fresh | BuiltinResultOwnership::Independent
        );
    if target.string_callback_operand_index().is_none() || !independent_result {
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

/// Roots by-value argument temporaries after evaluation and before the callee may unwind.
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
        if signature.is_some_and(|sig| sig.ref_params.get(index).copied().unwrap_or(false)) {
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
pub(super) fn root_owned_call_operand(
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

/// Clears a rooted operand before releasing it, including when its destructor throws.
pub(super) fn retire_owned_call_operand(
    ctx: &mut LoweringContext<'_, '_>,
    slot: crate::ir::LocalSlotId,
    span: Span,
) {
    ctx.emit_void(
        Op::PopCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PopCallOperandOwner.default_effects(), Some(span),
    );
    ctx.emit_void(
        Op::ReleaseLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(),
        Some(span),
    );
}
