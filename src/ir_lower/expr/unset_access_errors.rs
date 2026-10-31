//! Purpose:
//! Lowers catchable object-access errors for unset container fetches.
//!
//! Called from:
//! - Native dimension probes and staged unset property traversals.
//!
//! Key details:
//! - Native callable cells and eval-backed Closure objects share PHP's class identity.
//! - Interface checks remain eval-aware instead of relying only on dense native method tables.
//! - Callers keep receiver and converted-name owners published across these guards.

use super::*;

/// Recognizes compiled callable cells and logical Closure objects registered by eval.
pub(super) fn is_unset_closure(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, span: Span,
) -> LoweredValue {
    match ctx.builder.value_php_type(object.value).codegen_repr() {
        PhpType::Callable => emit_bool_literal(ctx, true, Some(span)),
        PhpType::Object(class) if class.trim_start_matches('\\').eq_ignore_ascii_case("Closure") =>
            emit_bool_literal(ctx, true, Some(span)),
        PhpType::Object(class) if class.is_empty() => {
            let target = crate::ir::RuntimeFnId::GetClass;
            let name = ctx.emit_value(Op::RuntimeCall, vec![object.value],
                Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))),
                PhpType::Str, target.effects(), Some(span));
            let closure = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral("Closure".into()), span));
            let result = ctx.emit_value(Op::StrictEq, vec![name.value, closure.value], None,
                PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
            crate::ir_lower::ownership::release_if_owned(ctx, name, Some(span));
            result
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let class = ctx.intern_class_name("Closure");
            let native = ctx.emit_value(Op::InstanceOf, vec![object.value], Some(Immediate::Data(class)),
                PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
            let known = ctx.builder.create_named_block("unset.closure.native", Vec::new());
            let lookup = ctx.builder.create_named_block("unset.closure.logical", Vec::new());
            let logical_object = ctx.builder.create_named_block("unset.closure.object", Vec::new());
            let not_object = ctx.builder.create_named_block("unset.closure.nonobject", Vec::new());
            let merge = ctx.builder.create_named_block("unset.closure.merge", Vec::new());
            let temp = ctx.declare_hidden_temp(PhpType::Bool);
            let initial = ctx.initialized_slots_snapshot();
            ctx.builder.terminate(Terminator::CondBr { cond: native.value,
                then_target: known, then_args: Vec::new(), else_target: lookup, else_args: Vec::new() });
            ctx.builder.position_at_end(known);
            let yes = emit_bool_literal(ctx, true, Some(span));
            store_value_into_temp(ctx, &temp, PhpType::Bool, yes, span);
            branch_to(ctx, merge);
            let known_slots = ctx.initialized_slots_snapshot();
            ctx.builder.position_at_end(lookup);
            ctx.restore_initialized_slots(initial.clone());
            let object_kind = ctx.emit_value(Op::TypePredicate, vec![object.value],
                Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
                Op::TypePredicate.default_effects(), Some(span));
            ctx.builder.terminate(Terminator::CondBr { cond: object_kind.value,
                then_target: logical_object, then_args: Vec::new(), else_target: not_object, else_args: Vec::new() });
            ctx.builder.position_at_end(logical_object);
            let class = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral("Closure".into()), span));
            let allow_string = emit_bool_literal(ctx, false, Some(span));
            let target = crate::ir::RuntimeFnId::IsA;
            let found = ctx.emit_value(Op::RuntimeCall, vec![object.value, class.value, allow_string.value],
                Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))),
                PhpType::Bool, target.effects(), Some(span));
            store_value_into_temp(ctx, &temp, PhpType::Bool, found, span);
            branch_to(ctx, merge);
            let lookup_slots = ctx.initialized_slots_snapshot();
            ctx.builder.position_at_end(not_object);
            ctx.restore_initialized_slots(initial.clone());
            let no = emit_bool_literal(ctx, false, Some(span));
            store_value_into_temp(ctx, &temp, PhpType::Bool, no, span);
            branch_to(ctx, merge);
            let nonobject_slots = ctx.initialized_slots_snapshot();
            ctx.builder.position_at_end(merge);
            let lookup_slots = merge_initialized_slots_for_expr(&initial,
                lookup_slots, true, nonobject_slots, true);
            ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial,
                known_slots, true, lookup_slots, true));
            take_owned_temp(ctx, &temp, span)
        }
        _ => emit_bool_literal(ctx, false, Some(span)),
    }
}

/// Checks public offset access before method dispatch so missing methods do not become null errors.
pub(super) fn require_unset_array_access(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, span: Span,
) {
    // The eval class-name bridge accepts object tag 6 but not native callable tag 10.
    let closure = is_unset_closure(ctx, object, span);
    let closure_error = ctx.builder.create_named_block("unset.array_access.closure", Vec::new());
    let check_interface = ctx.builder.create_named_block("unset.array_access.interface", Vec::new());
    let closure_initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: closure.value,
        then_target: closure_error, then_args: Vec::new(), else_target: check_interface, else_args: Vec::new() });
    ctx.builder.position_at_end(closure_error);
    crate::ir_lower::stmt::lower_throw_access_error(ctx, "Cannot use object of type Closure as array", span);
    ctx.builder.position_at_end(check_interface);
    ctx.restore_initialized_slots(closure_initial);
    let class = ctx.intern_class_name("ArrayAccess");
    let allowed = ctx.emit_value(Op::InstanceOf, vec![object.value], Some(Immediate::Data(class)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
    let valid = ctx.builder.create_named_block("unset.array_access.valid", Vec::new());
    let invalid = ctx.builder.create_named_block("unset.array_access.invalid", Vec::new());
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: allowed.value,
        then_target: valid, then_args: Vec::new(), else_target: invalid, else_args: Vec::new() });
    ctx.builder.position_at_end(invalid);
    let target = crate::ir::RuntimeFnId::GetClass;
    let name = ctx.emit_value(Op::RuntimeCall, vec![object.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))),
        PhpType::Str, target.effects(), Some(span));
    let temp = ctx.declare_owned_hidden_temp(PhpType::Str);
    store_value_into_temp(ctx, &temp, PhpType::Str, name, span);
    let slot = ctx.local_slots[&temp];
    register_owned_call_operand(ctx, slot, span);
    let name = Expr::new(ExprKind::Variable(temp), span);
    let message = access_message("Cannot use object of type ", name, " as array", span);
    crate::ir_lower::stmt::lower_throw_access_error_message(ctx, &message, span);
    ctx.builder.position_at_end(valid);
    ctx.restore_initialized_slots(initial);
}

/// Refuses a materializing property fetch on Closure after its name has been converted once.
pub(super) fn reject_unset_closure_property(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, name: &Expr, span: Span,
) {
    let is_object = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let check = ctx.builder.create_named_block("unset.property.object", Vec::new());
    let invalid = ctx.builder.create_named_block("unset.property.closure", Vec::new());
    let valid = ctx.builder.create_named_block("unset.property.nonclosure", Vec::new());
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: check, then_args: Vec::new(), else_target: valid, else_args: Vec::new() });
    ctx.builder.position_at_end(check);
    let closure = is_unset_closure(ctx, object, span);
    let checked_slots = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: closure.value,
        then_target: invalid, then_args: Vec::new(), else_target: valid, else_args: Vec::new() });
    ctx.builder.position_at_end(invalid);
    let message = access_message("Cannot create dynamic property Closure::$", name.clone(), "", span);
    crate::ir_lower::stmt::lower_throw_access_error_message(ctx, &message, span);
    ctx.builder.position_at_end(valid);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial,
        initial.clone(), true, checked_slots, true));
}

/// Builds an Error message from an already captured string without reevaluating its source.
fn access_message(prefix: &str, value: Expr, suffix: &str, span: Span) -> Expr {
    let message = Expr::new(ExprKind::BinaryOp {
        left: Box::new(Expr::new(ExprKind::StringLiteral(prefix.into()), span)),
        op: BinOp::Concat, right: Box::new(value),
    }, span);
    if suffix.is_empty() { return message; }
    Expr::new(ExprKind::BinaryOp { left: Box::new(message), op: BinOp::Concat,
        right: Box::new(Expr::new(ExprKind::StringLiteral(suffix.into()), span)),
    }, span)
}
