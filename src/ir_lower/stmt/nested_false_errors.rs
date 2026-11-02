//! Purpose:
//! Preserves PHP's deferred false-conversion exception completion and key-error priority.
//!
//! Called from:
//! - Plain nested stores, appends and intermediate write-context parent fetches.
//!
//! Key details:
//! - Callback exceptions are held in an unwind-visible slot until opcode completion.
//! - Int, bool and string dimensions complete; null, float and resource dimensions stop.
//! - Invalid array/object dimensions chain the handler exception under their TypeError.

use super::*;

/// One pending conversion exception and its separately tracked presence flag.
pub(super) struct PendingConversion {
    exception: String,
    flag: String,
}

/// Catches only the conversion callback while preserving all older write operand owners.
pub(super) fn capture(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, span: Span,
) -> PendingConversion {
    let exception = ctx.declare_owned_hidden_temp(PhpType::Object("Throwable".into()));
    ctx.clear_owned_hidden_temp(&exception, Some(span));
    let slot = ctx.local_slots[&exception];
    ctx.emit_void(Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PushCallOperandOwner.default_effects(), Some(span));
    let flag = ctx.declare_synthetic_php_local(PhpType::Bool);
    let absent = lower_expr(ctx, &Expr::new(ExprKind::BoolLiteral(false), span));
    ctx.store_local(&flag, absent, PhpType::Bool, Some(span));
    let initialized = ctx.initialized_slots_snapshot();
    let handler = ctx.builder.create_named_block("nested.false.capture", Vec::new());
    let after = ctx.builder.create_named_block("nested.false.captured", Vec::new());
    push_handler(ctx, handler, span);
    ctx.push_try_loop_depth();
    super::static_false_parent::normalize_for_update(ctx, parent, span);
    ctx.pop_try_loop_depth();
    super::exceptions::emit_try_pop_handler(ctx, handler.as_raw() as i64, span);
    branch_to(ctx, after);
    ctx.builder.position_at_end(handler);
    ctx.restore_initialized_slots(initialized);
    ctx.clear_static_callable_locals();
    super::exceptions::emit_try_pop_handler(ctx, handler.as_raw() as i64, span);
    let caught = ctx.emit_owned_value(Op::CatchBind, Vec::new(), None,
        PhpType::Object("Throwable".into()), Op::CatchBind.default_effects(), Some(span));
    ctx.store_call_argument_local(&exception, caught, PhpType::Object("Throwable".into()), Some(span));
    let present = lower_expr(ctx, &Expr::new(ExprKind::BoolLiteral(true), span));
    ctx.store_local(&flag, present, PhpType::Bool, Some(span));
    branch_to(ctx, after);
    ctx.builder.position_at_end(after);
    ctx.clear_static_callable_locals();
    PendingConversion { exception, flag }
}

/// Completes only eligible pending dimensions, then rethrows; returns on the no-error path.
pub(super) fn handle_pending(
    ctx: &mut LoweringContext<'_, '_>, pending: PendingConversion, key: Option<LoweredValue>,
    span: Span, action: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    let flag = ctx.load_local(&pending.flag, Some(span));
    let initialized = ctx.initialized_slots_snapshot();
    let raised = ctx.builder.create_named_block("nested.false.pending", Vec::new());
    let normal = ctx.builder.create_named_block("nested.false.normal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: flag.value, then_target: raised, then_args: Vec::new(),
        else_target: normal, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(raised);
    if let Some(key) = key {
        let invalid_array = predicate(ctx, key, crate::ir::PhpTypePredicate::Array, span);
        let invalid_object = predicate(ctx, key, crate::ir::PhpTypePredicate::Object, span);
        let invalid = or(ctx, invalid_array, invalid_object, span);
        let reject = ctx.builder.create_named_block("nested.false.invalid.key", Vec::new());
        let valid = ctx.builder.create_named_block("nested.false.valid.key", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: invalid.value, then_target: reject, then_args: Vec::new(),
            else_target: valid, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(reject);
        complete_and_rethrow(ctx, &pending, span, |ctx| {
            raise_invalid_key_type_error(ctx, key, span);
        });
        ctx.builder.position_at_end(valid);
        let int = predicate(ctx, key, crate::ir::PhpTypePredicate::Int, span);
        let boolean = predicate(ctx, key, crate::ir::PhpTypePredicate::Bool, span);
        let string = predicate(ctx, key, crate::ir::PhpTypePredicate::String, span);
        let scalar = or(ctx, int, boolean, span);
        let completes = or(ctx, scalar, string, span);
        let complete = ctx.builder.create_named_block("nested.false.complete", Vec::new());
        let stop = ctx.builder.create_named_block("nested.false.stop", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: completes.value, then_target: complete, then_args: Vec::new(),
            else_target: stop, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(stop);
        rethrow(ctx, &pending, span);
        ctx.builder.position_at_end(complete);
    }
    complete_and_rethrow(ctx, &pending, span, action);
    ctx.builder.position_at_end(normal);
    ctx.restore_initialized_slots(initialized);
    let slot = ctx.local_slots[&pending.exception];
    ctx.emit_void(Op::PopCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PopCallOperandOwner.default_effects(), Some(span));
}

/// Catches a completion error to append the original callback exception before propagating it.
fn complete_and_rethrow(
    ctx: &mut LoweringContext<'_, '_>, pending: &PendingConversion, span: Span,
    action: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    let handler = ctx.builder.create_named_block("nested.false.chain", Vec::new());
    push_handler(ctx, handler, span);
    ctx.push_try_loop_depth();
    action(ctx);
    ctx.pop_try_loop_depth();
    if !ctx.builder.insertion_block_is_terminated() {
        super::exceptions::emit_try_pop_handler(ctx, handler.as_raw() as i64, span);
        rethrow(ctx, pending, span);
    }
    ctx.builder.position_at_end(handler);
    ctx.clear_static_callable_locals();
    super::exceptions::emit_try_pop_handler(ctx, handler.as_raw() as i64, span);
    let escaped = ctx.emit_owned_value(Op::CatchBind, Vec::new(), None,
        PhpType::Object("Throwable".into()), Op::CatchBind.default_effects(), Some(span));
    let original = take_exception(ctx, pending, span);
    ctx.emit_void(Op::RuntimeCall, vec![escaped.value, original.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionChain)),
        crate::ir::Effects::all(), Some(span));
    terminate_throw(ctx, escaped.value);
}

/// Builds the PHP array/object key TypeError without allocating an unconsumed stored-value box.
fn raise_invalid_key_type_error(
    ctx: &mut LoweringContext<'_, '_>, key: LoweredValue, span: Span,
) {
    let ty = ctx.builder.value_php_type(key.value);
    let name = ctx.declare_synthetic_php_local(ty.clone());
    let held = crate::ir_lower::ownership::acquire_if_refcounted(ctx, key, Some(span));
    ctx.store_call_argument_local(&name, held, ty, Some(span));
    let slot = ctx.local_slots[&name];
    ctx.emit_void(Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PushCallOperandOwner.default_effects(), Some(span));
    let key = Expr::new(ExprKind::Variable(name), span);
    let kind = Expr::new(ExprKind::Ternary {
        condition: Box::new(Expr::new(ExprKind::FunctionCall {
            name: crate::names::Name::unqualified("is_array"), args: vec![key.clone()],
        }, span)),
        then_expr: Box::new(Expr::new(ExprKind::StringLiteral("array".into()), span)),
        else_expr: Box::new(Expr::new(ExprKind::FunctionCall {
            name: crate::names::Name::unqualified("get_class"), args: vec![key],
        }, span)),
    }, span);
    let message = Expr::new(ExprKind::BinaryOp {
        left: Box::new(Expr::new(ExprKind::BinaryOp {
            left: Box::new(Expr::new(ExprKind::StringLiteral("Cannot access offset of type ".into()), span)),
            op: crate::parser::ast::BinOp::Concat, right: Box::new(kind),
        }, span)),
        op: crate::parser::ast::BinOp::Concat,
        right: Box::new(Expr::new(ExprKind::StringLiteral(" on array".into()), span)),
    }, span);
    let error = lower_expr(ctx, &Expr::new(ExprKind::NewObject {
        class_name: crate::names::Name::unqualified("TypeError"), args: vec![message],
    }, span));
    crate::ir_lower::expr::retire_owned_call_operand(ctx, slot, span);
    terminate_throw(ctx, error.value);
}

/// Transfers the captured reference out of its guarded slot before throwing it again.
fn take_exception(
    ctx: &mut LoweringContext<'_, '_>, pending: &PendingConversion, span: Span,
) -> LoweredValue {
    let original = ctx.load_local(&pending.exception, Some(span));
    ctx.clear_owned_hidden_temp(&pending.exception, Some(span));
    let slot = ctx.local_slots[&pending.exception];
    ctx.emit_void(Op::PopCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PopCallOperandOwner.default_effects(), Some(span));
    original
}

/// Propagates the captured callback exception after completing or skipping the dimension.
fn rethrow(ctx: &mut LoweringContext<'_, '_>, pending: &PendingConversion, span: Span) {
    let original = take_exception(ctx, pending, span);
    terminate_throw(ctx, original.value);
}

/// Starts a short internal handler without introducing PHP-visible locals or exits.
fn push_handler(ctx: &mut LoweringContext<'_, '_>, handler: BlockId, span: Span) {
    ctx.emit_void(Op::TryPushHandler, Vec::new(), Some(Immediate::I64(handler.as_raw() as i64)),
        Op::TryPushHandler.default_effects(), Some(span));
}

/// Tests one PHP key kind through the existing target-neutral predicate.
fn predicate(
    ctx: &mut LoweringContext<'_, '_>, key: LoweredValue,
    predicate: crate::ir::PhpTypePredicate, span: Span,
) -> LoweredValue {
    ctx.emit_value(Op::TypePredicate, vec![key.value], Some(Immediate::TypePredicate(predicate)),
        PhpType::Bool, Op::TypePredicate.default_effects(), Some(span))
}

/// Combines admissible key classes without evaluating the key again.
fn or(
    ctx: &mut LoweringContext<'_, '_>, left: LoweredValue, right: LoweredValue, span: Span,
) -> LoweredValue {
    ctx.emit_value(Op::IBitOr, vec![left.value, right.value], None, PhpType::Bool,
        Op::IBitOr.default_effects(), Some(span))
}
