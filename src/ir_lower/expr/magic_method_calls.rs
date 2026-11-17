//! Purpose:
//! Builds PHP magic-method argument containers as unwind-visible EIR owners.
//!
//! Called from:
//! - Ordinary and runtime-selected native instance method lowering.
//!
//! Key details:
//! - The shared descriptor argument builder preserves numeric and named unpack keys.
//! - Arguments are passed by value, and the hook receives a generic boxed PHP array.

use super::*;

/// Forwards a missing method on a selected native class through the ordinary `__call` ABI.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str,
    args: &[Expr], op: Op, expr: &Expr,
) -> Option<LoweredValue> {
    if method_signature(ctx, object.value, method).is_some() { return None; }
    let object_ty = ctx.builder.value_php_type(object.value);
    let (class, _) = singular_object_class(&object_ty)?;
    let signature = class_method_signature(ctx, class, "__call")?.clone();
    let span = expr.span;
    let result_owner = prepublish_call_result(ctx, &signature.return_type, span);
    let (object, receiver_owner) = root_method_receiver(ctx, object, span);
    emit_native_factory_method_lookup(ctx, object, method, span);
    // A missing signature requests invoker reference markers for variables. Magic
    // arguments are values, so the hook's by-value passing modes must disable those markers.
    let arguments = lower_descriptor_invoker_arg_container_for_call_user_func(
        ctx, args, Some(&signature), false, span,
    );
    let slot = ctx.declare_synthetic_php_local(PhpType::Mixed);
    store_value_into_temp(ctx, &slot, PhpType::Mixed, arguments, span);
    let argument_owner = ctx.local_slots[&slot];
    register_owned_call_operand(ctx, argument_owner, span);
    let magic_args = [
        Expr::new(ExprKind::StringLiteral(method.to_string()), span),
        Expr::new(ExprKind::Variable(slot), span),
    ];
    // The outer slot owns this receiver. The nested ordinary call needs a borrowed view
    // so rooting its receiver cannot consume the outer slot's reference a second time.
    let receiver = ctx.emit_value(Op::Borrow, vec![object.value], None,
        ctx.builder.value_php_type(object.value), Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
    let call = super::nullable_method_calls::lower_method_call_with_receiver_direct(
        ctx, receiver, "__call", &magic_args, op, expr,
    );
    stage_call_result(ctx, result_owner.as_ref(), call, span);
    retire_owned_call_operand(ctx, argument_owner, span);
    if let Some(owner) = receiver_owner { retire_owned_call_operand(ctx, owner, span); }
    Some(take_prepublished_call_result(ctx, result_owner, call, span))
}
