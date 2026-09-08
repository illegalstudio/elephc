//! Purpose:
//! Prepares concrete date-family calls made through DateTimeInterface receivers.
//!
//! Called from:
//! - The ordinary and already-evaluated receiver method lowerers.
//!
//! Key details:
//! - Descendants are selected before ancestors, then the shared argument planner
//!   supplies the selected signature's defaults and variadic storage.

use super::*;

/// Dispatches before argument preparation so legal overrides can extend the ABI.
pub(super) fn lower_date_interface_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    op: Op,
    expr: &Expr,
) -> Option<LoweredValue> {
    if !matches!(op, Op::MethodCall | Op::NullsafeMethodCall) {
        return None;
    }
    let receiver_type = ctx.builder.value_php_type(object.value);
    let native_mutator = crate::types::date_method_dispatch::native_procedural_mutator_method(
        ctx.owner_name(),
    ).is_some_and(|native| native.eq_ignore_ascii_case(method));
    let native_procedural = native_mutator || crate::types::date_method_dispatch::native_procedural_read_method(
        ctx.owner_name(),
    ).is_some_and(|native| native.eq_ignore_ascii_case(method));
    let signature = if native_procedural {
        class_method_signature(ctx, "DateTime", &php_symbol_key(method))?.clone()
    } else {
        let (receiver, _) = singular_object_class(&receiver_type)?;
        crate::types::date_method_dispatch::concrete_date_interface_method(ctx.classes, receiver, method)?
    };
    let mut candidates = Vec::new();
    for name in ctx.classes.keys() {
        if native_mutator && name != "DateTime" {
            continue;
        }
        if native_procedural && !matches!(name.as_str(), "DateTime" | "DateTimeImmutable") {
            continue;
        }
        let mut ancestor = name.as_str();
        let mut depth = 0usize;
        loop {
            if matches!(ancestor, "DateTime" | "DateTimeImmutable") {
                candidates.push((depth, name.clone()));
                break;
            }
            let Some(parent) = ctx.classes.get(ancestor).and_then(|info| info.parent.as_deref()) else {
                break;
            };
            ancestor = parent;
            depth += 1;
        }
    }
    candidates.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
    let receiver_owner = ctx.declare_owned_hidden_temp(receiver_type.clone());
    store_value_into_temp(ctx, &receiver_owner, receiver_type, object, expr.span);
    let object = ctx.load_local(&receiver_owner, Some(expr.span));
    let result_type = normalize_value_php_type(signature.return_type);
    let result_temp = ctx.declare_owned_hidden_temp(result_type.clone());
    let merge = ctx.builder.create_named_block("date.interface.merge", Vec::new());
    for (_, class_name) in candidates {
        let matched = ctx.builder.create_named_block("date.interface.match", Vec::new());
        let next = ctx.builder.create_named_block("date.interface.next", Vec::new());
        let class_data = ctx.intern_class_name(&class_name);
        let condition = ctx.emit_value(
            Op::InstanceOf, vec![object.value], Some(Immediate::Data(class_data)),
            PhpType::Bool, Op::InstanceOf.default_effects(), Some(expr.span),
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: condition.value, then_target: matched, then_args: Vec::new(),
            else_target: next, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(matched);
        let concrete_type = PhpType::Object(class_name.clone());
        let concrete = if object.ir_type == IrType::Heap(IrHeapKind::Mixed) {
            let owned_box = crate::ir_lower::ownership::acquire_if_refcounted(
                ctx, object, Some(expr.span),
            );
            let data = ctx.intern_string(&format!("{}\0Invalid DateTimeInterface receiver: ",
                match &concrete_type { PhpType::Object(name) => name, _ => unreachable!() }));
            ctx.emit_value(Op::ReturnBoundaryMixedToObject, vec![owned_box.value],
                Some(Immediate::Data(data)), concrete_type,
                Op::ReturnBoundaryMixedToObject.default_effects(), Some(expr.span))
        } else {
            let receiver_temp = ctx.declare_owned_hidden_temp(concrete_type.clone());
            // This load borrows receiver_owner even though an OwnedTemp load is normally
            // consumed by store_value_into_temp. Keep that owner until the common exit.
            let retained = crate::ir_lower::ownership::acquire_if_refcounted(
                ctx, object, Some(expr.span),
            );
            ctx.store_local(&receiver_temp, retained, concrete_type, Some(expr.span));
            take_owned_temp(ctx, &receiver_temp, expr.span)
        };
        let call = if native_procedural {
            super::reflection_method_calls::lower_exact_reflection_instance_method(
                ctx, &class_name, method, concrete, args, result_type.clone(), false, expr,
            )
        } else {
            lower_method_call_with_receiver(ctx, concrete, method, args, Op::MethodCall, expr)
        };
        if !ctx.builder.insertion_block_is_terminated() {
            store_value_into_temp(ctx, &result_temp, result_type.clone(), call, expr.span);
            branch_to(ctx, merge);
        }
        ctx.builder.position_at_end(next);
    }
    if native_procedural {
        emit_procedural_receiver_type_error(ctx, &receiver_owner, expr.span);
    } else {
        let message = ctx.intern_string("Invalid DateTimeInterface receiver");
        ctx.emit_void(Op::ThrowError, Vec::new(), Some(Immediate::Data(message)),
            Op::ThrowError.default_effects(), Some(expr.span));
    }
    ctx.builder.terminate(Terminator::Unreachable);
    ctx.builder.position_at_end(merge);
    let receiver = take_owned_temp(ctx, &receiver_owner, expr.span);
    release_owning_receiver_temporary(ctx, receiver, expr.span);
    Some(take_owned_temp(ctx, &result_temp, expr.span))
}

/// Emits PHP's procedural argument error without invoking methods on an invalid receiver.
fn emit_procedural_receiver_type_error(ctx: &mut LoweringContext<'_, '_>, receiver_owner: &str, span: Span) {
    use crate::synthetic_class::{e_binop, e_str};
    let function = ctx.owner_name().strip_prefix("DateTime::__elephc_")
        .expect("a reserved procedural wrapper owns this error path").to_string();
    let parameter = crate::types::reflection_builtin_function_sig(&function)
        .and_then(|signature| signature.params.into_iter().next())
        .expect("a procedural date wrapper has a PHP receiver parameter").0;
    // Read the borrowed wrapper parameter, not a load that consumes the retained
    // receiver temporary. The diagnostic must finish before that owner is released.
    let actual = super::object_argument_guards::argument_type_name(&parameter, span);
    let receiver_type = if crate::types::date_method_dispatch::native_procedural_mutator_method(
        ctx.owner_name(),
    ).is_some() { "DateTime" } else { "DateTimeInterface" };
    let message = e_binop(e_str(&format!(
        "{function}(): Argument #1 (${parameter}) must be of type {receiver_type}, "
    )), BinOp::Concat, e_binop(actual, BinOp::Concat, e_str(" given")));
    let error = build_exception_from_expr(ctx, "TypeError", message, span);
    let receiver = take_owned_temp(ctx, receiver_owner, span);
    release_owning_receiver_temporary(ctx, receiver, span);
    ctx.emit_void(Op::ThrowException, vec![error.value], None,
        Op::ThrowException.default_effects(), Some(span));
}
