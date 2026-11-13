//! Purpose:
//! Selects concrete native enum signatures before materializing interface-call arguments.
//!
//! Called from:
//! - Regular and guarded instance-method expression lowering.
//!
//! Key details:
//! - The receiver is evaluated once; only the selected branch evaluates source arguments.
//! - Selected calls validate concrete parameter types before ordinary reference-return adoption.
//! - Interface declarations remain unchanged for reflection and non-enum fallback dispatch.
//! - Rejected binding throws only in its selected branch after source argument evaluation.
//! - Result-owner slots are zeroed before publication even when every branch rejects binding.

use super::*;
use super::nullable_method_calls::lower_method_call_with_receiver_direct;

/// Uses a final enum's signature, rather than its interface's defaults, for its runtime branch.
pub(super) fn lower_enum_interface_call(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str,
    args: &[Expr], op: Op, expr: &Expr,
) -> Option<LoweredValue> {
    if !matches!(op, Op::MethodCall | Op::NullsafeMethodCall) { return None; }
    let object_type = ctx.builder.value_php_type(object.value);
    let (interface, _) = singular_object_class(&object_type)?;
    if !ctx.interfaces.contains_key(interface) { return None; }
    let mut candidates: Vec<_> = ctx.classes.iter()
        .filter(|(name, info)| ctx.enums.contains_key(*name)
            && info.interfaces.iter().any(|name| name == interface))
        .map(|(name, info)| (info.class_id, name.clone())).collect();
    if candidates.is_empty() { return None; }
    candidates.sort();
    let has_class_fallback = ctx.classes.iter().any(|(name, info)|
        !ctx.enums.contains_key(name) && info.interfaces.iter().any(|name| name == interface));
    let result_type = method_call_result_type(ctx, object.value, method, op, expr);
    let reference_assignment = ctx.reference_call_context.as_ref()
        .is_some_and(|context| context.depth == ctx.expression_depth);
    let dynamic_unpack = args.iter().any(is_spread_arg)
        && crate::types::call_args::expand_planned_positional_spreads(args).is_none();
    let result_needs_owner = Ownership::php_type_needs_lifetime_tracking(&result_type)
        && !matches!(result_type.codegen_repr(), PhpType::Buffer(_));
    let result = if reference_assignment { None } else {
        let name = ctx.declare_owned_hidden_temp(result_type.clone());
        // Heap cleanup treats its slot as a pointer. Scalar result slots are neither heap
        // owners nor zero-initialized, and a rejected branch never stores a result there.
        if result_needs_owner {
            ctx.emit_void(Op::ZeroLocalSlot, Vec::new(),
                Some(Immediate::LocalSlot(ctx.local_slots[&name])),
                Op::ZeroLocalSlot.default_effects(), Some(expr.span));
            register_owned_call_operand(ctx, ctx.local_slots[&name], expr.span);
        }
        Some(name)
    };
    let (object, owner) = root_owned_call_operand(ctx, object, expr.span);
    let merge = ctx.builder.create_named_block("enum.interface.merge", Vec::new());
    for (_, class) in candidates {
        let selected = ctx.builder.create_named_block("enum.interface.selected", Vec::new());
        let next = ctx.builder.create_named_block("enum.interface.next", Vec::new());
        let data = ctx.intern_class_name(&class);
        let matches = ctx.emit_value(Op::InstanceOf, vec![object.value],
            Some(Immediate::Data(data)), PhpType::Bool, Op::InstanceOf.default_effects(), Some(expr.span));
        ctx.builder.terminate(Terminator::CondBr {
            cond: matches.value, then_target: selected, then_args: Vec::new(),
            else_target: next, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(selected);
        let signature = ctx.classes.get(&class)
            .and_then(|info| info.methods.get(&php_symbol_key(method))).cloned();
        if let Some(signature) = signature.as_ref() {
            if reference_assignment && signature.by_ref_return
                && !signature.return_type.reference_payload_compatible(&result_type)
            {
                crate::ir_lower::diagnostics::refuse(expr.span,
                    "Unsupported reference assignment: the selected enum reference payload \
                     differs from the interface result representation");
            }
            if super::selected_method_arguments::reject_invalid(
                ctx, &class, method, signature, args, expr.span,
            ) {
                ctx.builder.position_at_end(next);
                continue;
            }
        }
        let concrete = ctx.emit_value(Op::Borrow, vec![object.value], None,
            PhpType::Object(class), Op::Borrow.default_effects(), Some(expr.span));
        let uses_eir_binding = signature.as_ref()
            .is_some_and(|signature| !signature.ref_params.iter().any(|by_ref| *by_ref));
        if reference_assignment {
            ctx.reference_call_context.as_mut().unwrap().adopted = false;
        }
        let call = if dynamic_unpack && !reference_assignment && !uses_eir_binding {
            // Runtime unpacks can carry either packed values or named/sparse hash keys. The
            // ordinary descriptor binder owns those rules and catchable missing-argument errors.
            let callable = lower_instance_callable_from_receiver(ctx, concrete, method, expr.span);
            let callable = root_descriptor_callback(
                ctx, callable, descriptor_invoker_result_type(signature.as_ref()), expr.span,
            );
            let arguments = lower_descriptor_invoker_arg_container_for_call_user_func(
                ctx, args, signature.as_ref(), true, expr.span,
            );
            emit_callable_descriptor_invoke(ctx, callable, arguments, expr.span)
        } else {
            super::nullable_method_calls::lower_selected_enum_method_call(
                ctx, concrete, method, args, op, expr,
            )
        };
        complete_reference_branch(ctx, call, reference_assignment, &result_type, expr.span);
        if let Some(result) = &result {
            store_value_into_temp(ctx, result, result_type.clone(), call, expr.span);
        }
        branch_to(ctx, merge);
        ctx.builder.position_at_end(next);
    }
    if has_class_fallback {
        let fallback = ctx.emit_value(Op::Borrow, vec![object.value], None,
            object_type, Op::Borrow.default_effects(), Some(expr.span));
        if reference_assignment {
            ctx.reference_call_context.as_mut().unwrap().adopted = false;
        }
        let call = lower_method_call_with_receiver_direct(ctx, fallback, method, args, op, expr);
        // A class interface adapter can erase an added reference return. Leave a
        // non-adopting fallback false so the enclosing assignment refuses that shape.
        if let Some(result) = &result {
            store_value_into_temp(ctx, result, result_type, call, expr.span);
        }
        branch_to(ctx, merge);
    } else {
        // No native class implements this interface. Do not emit an impossible
        // declaration-shaped call that would reintroduce its required/default slots.
        let exception = Expr::new(ExprKind::NewObject {
            class_name: Name::unqualified("Error"),
            args: vec![Expr::new(ExprKind::StringLiteral(
                format!("Unknown native implementation of {interface}::{method}")
            ), expr.span)],
        }, expr.span);
        let exception = lower_expr(ctx, &exception);
        ctx.builder.terminate(Terminator::Throw { value: exception.value });
    }
    ctx.builder.position_at_end(merge);
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, expr.span); }
    Some(if let Some(result) = result {
        if result_needs_owner {
            unregister_owned_call_operand(ctx, ctx.local_slots[&result], expr.span);
        }
        take_owned_temp(ctx, &result, expr.span)
    } else { lower_null(ctx, expr) })
}

/// Gives a selected by-value result its own cell instead of another branch's return lease.
/// The enclosing reference assignment already published staging for unwind cleanup.
fn complete_reference_branch(
    ctx: &mut LoweringContext<'_, '_>, call: LoweredValue, reference_assignment: bool,
    result_type: &PhpType, span: Span,
) {
    if !reference_assignment || ctx.reference_call_context.as_ref().unwrap().adopted { return; }
    let staged = ctx.reference_call_context.as_ref().unwrap().staged.clone();
    let temporary = ctx.declare_synthetic_php_local(result_type.clone());
    store_value_into_temp(ctx, &temporary, result_type.clone(), call, span);
    ctx.promote_local_ref_cell(&temporary, Some(span));
    ctx.alias_local_ref_cell(&staged, &temporary, Some(span));
    ctx.release_ref_cell_owner(&temporary, Some(span));
    ctx.reference_call_context.as_mut().unwrap().adopted = true;
}
