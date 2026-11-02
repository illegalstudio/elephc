//! Purpose:
//! Lowers associative array_push unpacks without a user-function variadic collector.
//!
//! Called from:
//! - Direct builtin call lowering after non-local receiver preparation.
//!
//! Key details:
//! - Argument snapshots and unpack validation precede all receiver mutations.
//! - Iterator and argument owners remain visible to same-frame exception cleanup.

use super::*;

/// Expands runtime unpack sources into individual appends to a hash or boxed array receiver.
pub(super) fn lower_array_push_spread(
    ctx: &mut LoweringContext<'_, '_>, name: &Name, args: &[Expr], expr: &Expr,
) -> Option<LoweredValue> {
    let canonical = name.as_str();
    if ctx.functions.contains_key(canonical) || ctx.extern_functions.contains_key(canonical)
        || !crate::builtins::registry::lookup(canonical).is_some_and(|def| matches!(
            def.spec.semantics.lowering,
            crate::builtins::semantics::BuiltinLowering::Runtime(
                crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ArrayPush))))
        || !args.iter().any(is_spread_arg) || crate::types::call_args::has_named_args(args)
    { return None; }
    let ExprKind::Variable(receiver) = &args.first()?.kind else { return None; };
    if !matches!(ctx.local_type(receiver).codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_))
    { return None; }
    let sig = crate::types::builtin_call_sig(canonical)?;
    let plan = crate::types::call_args::plan_call_args(&sig, args, expr.span, false, false).ok()?;
    begin_call_argument_evaluation(ctx);
    let named = ctx.declare_synthetic_php_local(PhpType::Bool);
    let no_names = lower_expr(ctx, &Expr::new(ExprKind::BoolLiteral(false), expr.span));
    ctx.store_local(&named, no_names, PhpType::Bool, Some(expr.span));
    let names = stage_value(ctx, &Expr::new(ExprKind::ArrayLiteralAssoc(vec![(
        Expr::new(ExprKind::StringLiteral(sig.params[0].0.clone()), expr.span),
        Expr::new(ExprKind::BoolLiteral(true), expr.span),
    )]), expr.span));
    let mut staged = Vec::new();
    for arg in plan.source_args.iter().skip(1) {
        let (spread, source) = match &arg.kind {
            ExprKind::Spread(source) => (true, source.as_ref()),
            _ => (false, arg),
        };
        let slot = if spread {
            stage_spread(ctx, source, &named, &names, expr.span)
        } else { stage_value(ctx, source) };
        staged.push((spread, slot));
    }
    let has_names = ctx.load_local(&named, Some(expr.span));
    require(ctx, has_names, "ArgumentCountError",
        "array_push() does not accept unknown named parameters", false, expr.span);
    for (spread, slot) in &staged {
        if *spread {
            if matches!(ctx.local_type(slot).codegen_repr(), PhpType::Array(element)
                if matches!(*element, PhpType::Void | PhpType::Never)) { continue; }
            walk_source(ctx, slot, expr.span, |ctx, iterator| {
                let source_ty = ctx.local_type(slot).codegen_repr();
                let value_ty = match source_ty {
                    PhpType::Array(element) => *element,
                    PhpType::AssocArray { value, .. } => *value,
                    _ => PhpType::Mixed,
                };
                let value = ctx.emit_value(Op::IterCurrentValue, vec![iterator.value], None,
                    value_ty, Op::IterCurrentValue.default_effects(), Some(expr.span));
                append_value(ctx, canonical, receiver, value, expr.span);
            });
        } else {
            let value = ctx.load_local(slot, Some(expr.span));
            append_value(ctx, canonical, receiver, value, expr.span);
        }
    }
    let array = ctx.load_local(receiver, Some(expr.span));
    let result = emit_builtin_call_value(ctx, canonical, vec![array.value], PhpType::Int, expr.span, None);
    Some(finish_scope(ctx, result))
}

/// Publishes a by-value snapshot before later argument effects can replace its source.
pub(super) fn stage_value(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> String {
    let value = lower_expr(ctx, expr);
    let ty = ctx.builder.value_php_type(value.value);
    let value = if ty.codegen_repr() == PhpType::Mixed {
        let copy = ctx.emit_owned_value(Op::MixedClone, vec![value.value], None, ty.clone(),
            Op::MixedClone.default_effects(), Some(expr.span));
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
        }
        copy
    } else { value };
    let temp = ctx.declare_synthetic_php_local(ty.clone());
    ctx.store_local(&temp, value, ty, Some(expr.span));
    publish_scoped_value_local(ctx, &temp, expr.span);
    temp
}

/// Retires call-scoped snapshots and receiver leases after stabilizing the scalar result.
pub(super) fn finish_scope(ctx: &mut LoweringContext<'_, '_>, result: LoweredValue) -> LoweredValue {
    let mut operands = vec![result.value];
    let owners = finish_call_argument_evaluation(ctx, &mut operands);
    retire_call_argument_intermediates(ctx, &owners);
    lowered_value_from_id(ctx, operands[0])
}

/// Copies each unpack value at its source point, before evaluating the next argument.
fn stage_spread(
    ctx: &mut LoweringContext<'_, '_>, expr: &Expr, named_slot: &str,
    names_slot: &str, span: Span,
) -> String {
    let source_slot = stage_value(ctx, expr);
    let source = ctx.load_local(&source_slot, Some(span));
    let null = ctx.emit_value(Op::IsNull, vec![source.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    require(ctx, null, "TypeError", "Only arrays and Traversables can be unpacked, null given", false, span);
    let iterable = ctx.emit_value(Op::TypePredicate, vec![source.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Iterable)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    require_iterable(ctx, iterable, &source_slot, span);
    let source_type = ctx.local_type(&source_slot).codegen_repr();
    let element = match source_type.clone() {
        PhpType::Array(element) => *element,
        PhpType::AssocArray { value, .. } => *value,
        _ => PhpType::Mixed,
    };
    let array_type = PhpType::Array(Box::new(element.clone()));
    let values_slot = ctx.declare_synthetic_php_local(array_type.clone());
    let values = ctx.emit_owned_value(Op::ArrayNew, Vec::new(), Some(Immediate::Capacity(4)),
        array_type.clone(), Op::ArrayNew.default_effects(), Some(span));
    ctx.store_local(&values_slot, values, array_type, Some(span));
    publish_scoped_value_local(ctx, &values_slot, span);
    let source_named = ctx.declare_synthetic_php_local(PhpType::Bool);
    let no_names = lower_expr(ctx, &Expr::new(ExprKind::BoolLiteral(false), span));
    ctx.store_local(&source_named, no_names, PhpType::Bool, Some(span));
    if !matches!(element, PhpType::Void | PhpType::Never) {
        walk_source(ctx, &source_slot, span, |ctx, iterator| {
            let fetched_type = if matches!(source_type, PhpType::AssocArray { .. }) {
                PhpType::Mixed
            } else { element.clone() };
            let value = ctx.emit_value(Op::IterCurrentValue, vec![iterator.value], None, fetched_type,
                Op::IterCurrentValue.default_effects(), Some(span));
            let value = coerce_value_for_temp(ctx, value, &element, span);
            let value = if element.codegen_repr() == PhpType::Mixed {
                let copied = ctx.emit_owned_value(Op::MixedClone, vec![value.value], None, element.clone(),
                    Op::MixedClone.default_effects(), Some(span));
                if ctx.value_is_owning_temporary(value) {
                    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
                }
                copied
            } else { value };
            let pins = pin_in_flight_owners(ctx, &[value.value], span);
            check_key(ctx, iterator, named_slot, &source_named, names_slot, span);
            let values = ctx.load_local(&values_slot, Some(span));
            ctx.emit_void(Op::ArrayPush, vec![values.value, value.value], None,
                Op::ArrayPush.default_effects(), Some(span));
            unpin_in_flight_owners(ctx, pins, span);
            if ctx.value_is_owning_temporary(value) {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
            }
        });
    }
    // The captured values now own their snapshots. A temporary iterator source may be destroyed
    // here, at the unpack's evaluation point, while the empty unwind record stays balanced.
    let slot = ctx.local_slots[&source_slot];
    ctx.emit_void(Op::ReleaseLocalSlot, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(), Some(span));
    values_slot
}

/// Consumes planned fixed-prefix occupancy while checking keys of a runtime variadic tail.
fn check_key(
    ctx: &mut LoweringContext<'_, '_>, iterator: LoweredValue, named_slot: &str,
    source_named: &str, names_slot: &str, span: Span,
) {
    let key = ctx.emit_value(Op::IterCurrentKey, vec![iterator.value], None, PhpType::Mixed,
        Op::IterCurrentKey.default_effects(), Some(span));
    let pins = pin_in_flight_owners(ctx, &[key.value], span);
    let named = ctx.emit_value(Op::TypePredicate, vec![key.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::String)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let string = ctx.builder.create_named_block("push.unpack.string", Vec::new());
    let named_string = ctx.builder.create_named_block("push.unpack.named", Vec::new());
    let non_string = ctx.builder.create_named_block("push.unpack.non_string", Vec::new());
    let positional = ctx.builder.create_named_block("push.unpack.positional", Vec::new());
    let done = ctx.builder.create_named_block("push.unpack.key.done", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: named.value,
        then_target: string, then_args: Vec::new(), else_target: non_string, else_args: Vec::new() });
    ctx.builder.position_at_end(string);
    let positional_string = runtime_string_key_is_positional(ctx, key, span);
    ctx.builder.terminate(Terminator::CondBr { cond: positional_string.value,
        then_target: positional, then_args: Vec::new(), else_target: named_string, else_args: Vec::new() });
    ctx.builder.position_at_end(named_string);
    let names = ctx.load_local(names_slot, Some(span));
    let duplicate = ctx.emit_value(Op::DescriptorArgKeyExists, vec![names.value, key.value], None,
        PhpType::Bool, Op::DescriptorArgKeyExists.default_effects(), Some(span));
    let occupied = ctx.builder.create_named_block("push.unpack.duplicate", Vec::new());
    let available = ctx.builder.create_named_block("push.unpack.name.free", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: duplicate.value,
        then_target: occupied, then_args: Vec::new(), else_target: available, else_args: Vec::new() });
    ctx.builder.position_at_end(occupied);
    ctx.emit_void(Op::ThrowNamedParameterOverwrite, vec![key.value], None,
        Op::ThrowNamedParameterOverwrite.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::Unreachable);
    ctx.builder.position_at_end(available);
    let yes = lower_expr(ctx, &Expr::new(ExprKind::BoolLiteral(true), span));
    ctx.emit_void(Op::DescriptorArgSet, vec![names.value, key.value, yes.value], None,
        Op::DescriptorArgSet.default_effects(), Some(span));
    ctx.store_local(named_slot, yes, PhpType::Bool, Some(span));
    ctx.store_local(source_named, yes, PhpType::Bool, Some(span));
    ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    ctx.builder.position_at_end(non_string);
    let integer_key = ctx.emit_value(Op::TypePredicate, vec![key.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Int)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    require(ctx, integer_key, "Error", "Keys must be of type int|string during argument unpacking", true, span);
    ctx.builder.terminate(Terminator::Br { target: positional, args: Vec::new() });
    ctx.builder.position_at_end(positional);
    let named_before = ctx.load_local(source_named, Some(span));
    require(ctx, named_before, "Error", "Cannot use positional argument after named argument during unpacking", false, span);
    ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    ctx.builder.position_at_end(done);
    unpin_in_flight_owners(ctx, pins, span);
    if ctx.value_is_owning_temporary(key) {
        crate::ir_lower::ownership::release_if_owned(ctx, key, Some(span));
    }
}

/// Applies PHP array-key canonicalization to a runtime string without changing the key itself.
fn runtime_string_key_is_positional(
    ctx: &mut LoweringContext<'_, '_>, key: LoweredValue, span: Span,
) -> LoweredValue {
    let integer = ctx.emit_value(Op::Cast, vec![key.value],
        Some(Immediate::CastTarget(IrType::I64)), PhpType::Int,
        Op::Cast.default_effects(), Some(span));
    let round_trip = ctx.emit_value(Op::IToStr, vec![integer.value], None, PhpType::Str,
        Op::IToStr.default_effects(), Some(span));
    let canonical = ctx.emit_value(Op::StrictEq, vec![key.value, round_trip.value], None,
        PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
    if ctx.value_is_owning_temporary(round_trip) {
        crate::ir_lower::ownership::release_if_owned(ctx, round_trip, Some(span));
    }
    canonical
}

/// Walks an argument snapshot using the ordinary target-neutral iterator contract.
fn walk_source(
    ctx: &mut LoweringContext<'_, '_>, slot: &str, span: Span,
    mut body: impl FnMut(&mut LoweringContext<'_, '_>, LoweredValue),
) {
    let source = ctx.load_local(slot, Some(span));
    let (iterator, owner, _) = ctx.emit_iter_start(source, false, span);
    let header = ctx.builder.create_named_block("push.unpack.next", Vec::new());
    let value = ctx.builder.create_named_block("push.unpack.value", Vec::new());
    let exit = ctx.builder.create_named_block("push.unpack.done", Vec::new());
    ctx.builder.terminate(Terminator::Br { target: header, args: Vec::new() });
    ctx.builder.position_at_end(header);
    let next = ctx.emit_value(Op::IterNext, vec![iterator.value], None, PhpType::Bool,
        Op::IterNext.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::CondBr { cond: next.value,
        then_target: value, then_args: Vec::new(), else_target: exit, else_args: Vec::new() });
    ctx.builder.position_at_end(value);
    body(ctx, iterator);
    ctx.builder.terminate(Terminator::Br { target: header, args: Vec::new() });
    ctx.builder.position_at_end(exit);
    if let Some(owner) = owner { ctx.retire_iter_start_owner(owner, span); }
}

/// Appends one evaluated value through the builtin's ordinary ownership and unwind handling.
fn append_value(
    ctx: &mut LoweringContext<'_, '_>, canonical: &str, receiver: &str,
    value: LoweredValue, span: Span,
) {
    if try_append_indexed_array_local_value(ctx, receiver, value, span) {
        return;
    }
    let array = ctx.load_local(receiver, Some(span));
    emit_builtin_call_value(ctx, canonical, vec![array.value, value.value], PhpType::Int, span, None);
}

/// Raises PHP's dynamic unpack TypeError when a staged source is not iterable.
fn require_iterable(
    ctx: &mut LoweringContext<'_, '_>, condition: LoweredValue, source_slot: &str, span: Span,
) {
    let valid = ctx.builder.create_named_block("push.unpack.iterable", Vec::new());
    let invalid = ctx.builder.create_named_block("push.unpack.non_iterable", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value,
        then_target: valid,
        then_args: Vec::new(),
        else_target: invalid,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(invalid);
    let source = Expr::new(ExprKind::Variable(source_slot.to_string()), span);
    let message = super::enum_argument_guards::concat(
        Expr::new(ExprKind::StringLiteral(
            "Only arrays and Traversables can be unpacked, ".into(),
        ), span),
        super::enum_argument_guards::concat(
            super::enum_argument_guards::runtime_debug_type(&source, span),
            Expr::new(ExprKind::StringLiteral(" given".into()), span),
            span,
        ),
        span,
    );
    super::enum_argument_guards::throw_message(ctx, "TypeError", message, span);
    ctx.builder.position_at_end(valid);
}

/// Emits a catchable PHP argument error while preserving all outer argument owner records.
pub(super) fn require(ctx: &mut LoweringContext<'_, '_>, condition: LoweredValue, class: &str, message: &str, expected: bool, span: Span) {
    let valid = ctx.builder.create_named_block("push.unpack.valid", Vec::new());
    let invalid = ctx.builder.create_named_block("push.unpack.invalid", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: if expected { valid } else { invalid }, then_args: Vec::new(),
        else_target: if expected { invalid } else { valid }, else_args: Vec::new() });
    ctx.builder.position_at_end(invalid);
    let error = lower_expr(ctx, &Expr::new(ExprKind::NewObject {
        class_name: Name::unqualified(class),
        args: vec![Expr::new(ExprKind::StringLiteral(message.into()), span)],
    }, span));
    ctx.builder.terminate(Terminator::Throw { value: error.value });
    ctx.builder.position_at_end(valid);
}
