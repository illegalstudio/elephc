//! Purpose:
//! Lowers dynamic callable creation with catchable PHP errors and owned descriptors.
//!
//! Called from:
//! - Static method lowering for the parser marker and Closure::fromCallable.
//!
//! Key details:
//! - The source is evaluated and rooted once before validation and normalization.

use super::*;

/// Validates the selected callable before acquiring its returned descriptor owner.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, args: &[Expr], method: &str, expr: &Expr,
) -> LoweredValue {
    let result_name = ctx.declare_owned_hidden_temp(PhpType::Callable);
    let result_owner = ctx.local_slots[&result_name];
    register_owned_call_operand(ctx, result_owner, expr.span);
    let sig = crate::types::signatures::closure_factory_signature();
    let operands = lower_args_with_signature(ctx, Some(&sig), args);
    let value = LoweredValue { value: operands[0], ir_type: ctx.builder.value_type(operands[0]) };
    // A declared `callable` source resolves runtime names exactly as invoking it would.
    let declared_source = ctx.builder.value_php_type(value.value) == PhpType::DeclaredCallable
        || matches!(args, [arg] if super::declared_callables::expr_is_declared_callable(ctx, arg));
    let value = if matches!(ctx.builder.value_php_type(value.value).codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        value
    } else { ctx.box_value_as_mixed(value, PhpType::Mixed, Some(expr.span)) };
    let value = if ctx.value_is_owning_temporary(value) { value } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(expr.span))
    };
    let source_name = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    ctx.store_local(&source_name, value, PhpType::Mixed, Some(expr.span));
    let owner = ctx.local_slots[&source_name];
    register_owned_call_operand(ctx, owner, expr.span);
    let value = ctx.load_local(&source_name, Some(expr.span));
    let dynamic = method == crate::parser::ast::DYNAMIC_CALLABLE_FACTORY;
    let factory = if declared_source { Immediate::DeclaredCallableFactory(dynamic) } else { Immediate::Bool(dynamic) };
    let exception = if dynamic { "Error" } else { "TypeError" };
    let message = if dynamic { "Value is not callable" } else { "Failed to create closure from callable" };
    let target = crate::ir::RuntimeFnId::IsCallable;
    let valid = ctx.emit_value(Op::RuntimeCall, vec![value.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))),
        PhpType::Bool, target.effects(), Some(expr.span));
    // Public probes cannot decide class-scoped method access. Let descriptor selection
    // validate strings and arrays against the instruction's lexical class instead.
    let array_kind = ctx.emit_value(Op::TypePredicate, vec![value.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Array)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(expr.span));
    let string_kind = ctx.emit_value(Op::TypePredicate, vec![value.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::String)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(expr.span));
    let method_shape = ctx.emit_value(Op::IBitOr, vec![array_kind.value, string_kind.value], None,
        PhpType::Bool, Op::IBitOr.default_effects(), Some(expr.span));
    let valid = ctx.emit_value(Op::IBitOr, vec![valid.value, method_shape.value], None,
        PhpType::Bool, Op::IBitOr.default_effects(), Some(expr.span));
    let accepted = ctx.builder.create_named_block("callable.accepted", Vec::new());
    let rejected = ctx.builder.create_named_block("callable.rejected", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: valid.value,
        then_target: accepted, then_args: Vec::new(), else_target: rejected, else_args: Vec::new() });
    ctx.builder.position_at_end(rejected);
    crate::ir_lower::stmt::lower_throw_access_exception(ctx, exception, message, expr.span);
    ctx.builder.position_at_end(accepted);
    // Canonicalize both indexed and associative callable pairs by key, not insertion order.
    // The non-array arm preserves the runtime object class for the shared __invoke ladder.
    let array_block = ctx.builder.create_named_block("callable.array", Vec::new());
    let other_block = ctx.builder.create_named_block("callable.other", Vec::new());
    let merge = ctx.builder.create_named_block("callable.normalized", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: array_kind.value,
        then_target: array_block, then_args: Vec::new(), else_target: other_block, else_args: Vec::new() });
    ctx.builder.position_at_end(array_block);
    let count_fn = crate::ir::RuntimeFnId::Count;
    let count = ctx.emit_value(Op::RuntimeCall, vec![value.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(count_fn))),
        PhpType::Int, count_fn.effects(), Some(expr.span));
    let two = lower_int_literal(ctx, 2, expr);
    let pair_count = ctx.emit_value(Op::ICmp, vec![count.value, two.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)), PhpType::Bool,
        Op::ICmp.default_effects(), Some(expr.span));
    let pair_block = ctx.builder.create_named_block("callable.pair", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: pair_count.value,
        then_target: pair_block, then_args: Vec::new(), else_target: rejected, else_args: Vec::new() });
    ctx.builder.position_at_end(pair_block);
    let pair_type = PhpType::Array(Box::new(PhpType::Mixed));
    let pair = ctx.emit_owned_value(Op::ArrayNew, Vec::new(), Some(Immediate::Capacity(2)),
        pair_type, Op::ArrayNew.default_effects(), Some(expr.span));
    let (pair, pair_owner) = root_closure_bind_operand(ctx, pair, expr.span);
    for key in [0, 1] {
        let index = lower_int_literal(ctx, key, expr);
        let warn = emit_bool_literal(ctx, false, Some(expr.span));
        let item = ctx.emit_owned_value(Op::RuntimeCall, vec![value.value, index.value, warn.value], None,
            PhpType::Mixed, Op::RuntimeCall.default_effects(), Some(expr.span));
        ctx.emit_void(Op::ArrayPush, vec![pair.value, item.value], None,
            Op::ArrayPush.default_effects(), Some(expr.span));
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, Some(&PhpType::Mixed), item, expr.span);
    }
    let boxed_pair = ctx.emit_owned_value(Op::MixedBox, vec![pair.value], None,
        PhpType::Mixed, Op::MixedBox.default_effects(), Some(expr.span));
    let (boxed_pair, boxed_owner) = root_closure_bind_operand(ctx, boxed_pair, expr.span);
    let descriptor = ctx.emit_owned_value(Op::NormalizeCallable, vec![boxed_pair.value], Some(factory.clone()),
        PhpType::Callable, Op::NormalizeCallable.default_effects(), Some(expr.span));
    ctx.store_local(&result_name, descriptor, PhpType::Callable, Some(expr.span));
    if let Some(owner) = boxed_owner { retire_owned_call_operand(ctx, owner, expr.span); }
    if let Some(owner) = pair_owner { retire_owned_call_operand(ctx, owner, expr.span); }
    branch_to(ctx, merge);
    ctx.builder.position_at_end(other_block);
    let descriptor = ctx.emit_owned_value(Op::NormalizeCallable, vec![value.value], Some(factory.clone()),
        PhpType::Callable, Op::NormalizeCallable.default_effects(), Some(expr.span));
    ctx.store_local(&result_name, descriptor, PhpType::Callable, Some(expr.span));
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    retire_owned_call_operand(ctx, owner, expr.span);
    let descriptor = ctx.load_local(&result_name, Some(expr.span));
    let descriptor = crate::ir_lower::ownership::acquire_if_refcounted(ctx, descriptor, Some(expr.span));
    retire_owned_call_operand(ctx, result_owner, expr.span);
    descriptor
}
