//! Purpose:
//! Lowers keyed writes through attached string and false parent cells.
//!
//! Called from:
//! - `crate::ir_lower::stmt::nested_array_writes::lower_nested_array_assign()`.
//!
//! Key details:
//! - Strings are rebuilt from target-neutral EIR operations and existing typed runtime calls.
//! - Caller-owned parent, key and RHS roots remain active across warning handlers and throws.

use super::*;
use crate::ir::RuntimeFnId;

/// Selects a string offset write or the existing array/object setter using the runtime parent.
pub(super) fn lower_nested_set(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue,
    key: LoweredValue, value: LoweredValue, span: Span, key_already_diagnosed: bool,
) {
    if ctx.builder.value_php_type(parent.value).codegen_repr() != PhpType::Mixed {
        emit_set(ctx, parent, key, value, span, key_already_diagnosed);
        return;
    }
    let string = predicate(ctx, parent, crate::ir::PhpTypePredicate::String, span);
    let string_write = ctx.builder.create_named_block("nested.write.string", Vec::new());
    let array_write = ctx.builder.create_named_block("nested.write.container", Vec::new());
    let done = ctx.builder.create_named_block("nested.write.done", Vec::new());
    let initialized = ctx.initialized_slots_snapshot();
    branch(ctx, string, string_write, array_write);
    ctx.builder.position_at_end(string_write);
    lower_string_write(ctx, StringWriteTarget::Cell(parent), parent, key, value, span);
    ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    ctx.builder.position_at_end(array_write);
    ctx.restore_initialized_slots(initialized.clone());
    let boolean = predicate(ctx, parent, crate::ir::PhpTypePredicate::Bool, span);
    let truth = ctx.emit_value(Op::IsTruthy, vec![parent.value], None, PhpType::Bool,
        Op::IsTruthy.default_effects(), Some(span));
    let zero = int(ctx, 0);
    let false_value = compare(ctx, truth, zero, CmpPredicate::Eq, span);
    let false_parent = ctx.emit_value(Op::IBitAnd, vec![boolean.value, false_value.value], None,
        PhpType::Bool, Op::IBitAnd.default_effects(), Some(span));
    let promote = ctx.builder.create_named_block("nested.write.false.promote", Vec::new());
    let store = ctx.builder.create_named_block("nested.write.store", Vec::new());
    branch(ctx, false_parent, promote, store);
    ctx.builder.position_at_end(promote);
    let empty = lower_expr(ctx, &Expr::new(ExprKind::ArrayLiteral(Vec::new()), span));
    replace_cell(ctx, parent, empty, span);
    crate::ir_lower::ownership::release_if_owned(ctx, empty, Some(span));
    lower_false_write(ctx, parent, key, value, done, span, key_already_diagnosed);
    ctx.builder.position_at_end(store);
    emit_set(ctx, parent, key, value, span, key_already_diagnosed);
    ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(initialized);
}

/// Warns after promotion but before insertion, deferring a handler exception as PHP does.
fn lower_false_write(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue,
    key: LoweredValue, value: LoweredValue, done: BlockId, span: Span,
    key_already_diagnosed: bool,
) {
    let caught = ctx.builder.create_named_block("nested.write.false.handler.throw", Vec::new());
    let token = caught.as_raw() as i64;
    ctx.emit_void(Op::TryPushHandler, Vec::new(), Some(Immediate::I64(token)),
        Op::TryPushHandler.default_effects(), Some(span));
    diagnostic(ctx, "Deprecated: Automatic conversion of false to array is deprecated\n", span);
    super::exceptions::emit_try_pop_handler(ctx, token, span);
    emit_set(ctx, parent, key, value, span, key_already_diagnosed);
    ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });

    ctx.builder.position_at_end(caught);
    ctx.clear_static_callable_locals();
    super::exceptions::emit_try_pop_handler(ctx, token, span);
    let throwable = PhpType::Object("Throwable".into());
    let pending_name = ctx.declare_owned_hidden_temp(throwable.clone());
    let pending = ctx.emit_owned_value(Op::CatchBind, Vec::new(), None, throwable.clone(),
        Op::CatchBind.default_effects(), Some(span));
    ctx.store_local(&pending_name, pending, throwable, Some(span));
    let pending_slot = ctx.local_slots[&pending_name];
    ctx.emit_void(Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(pending_slot)),
        Op::PushCallOperandOwner.default_effects(), Some(span));
    let mut direct = predicate(ctx, key, crate::ir::PhpTypePredicate::Int, span);
    for kind in [crate::ir::PhpTypePredicate::String, crate::ir::PhpTypePredicate::Bool] {
        let other = predicate(ctx, key, kind, span);
        direct = ctx.emit_value(Op::IBitOr, vec![direct.value, other.value], None,
            PhpType::Bool, Op::IBitOr.default_effects(), Some(span));
    }
    let insert = ctx.builder.create_named_block("nested.write.false.pending.insert", Vec::new());
    let check = ctx.builder.create_named_block("nested.write.false.pending.key", Vec::new());
    let rethrow = ctx.builder.create_named_block("nested.write.false.pending.rethrow", Vec::new());
    branch(ctx, direct, insert, check);
    ctx.builder.position_at_end(insert);
    emit_set(ctx, parent, key, value, span, key_already_diagnosed);
    ctx.builder.terminate(Terminator::Br { target: rethrow, args: Vec::new() });
    ctx.builder.position_at_end(check);
    let array = predicate(ctx, key, crate::ir::PhpTypePredicate::Array, span);
    let object = predicate(ctx, key, crate::ir::PhpTypePredicate::Object, span);
    let invalid = ctx.emit_value(Op::IBitOr, vec![array.value, object.value], None,
        PhpType::Bool, Op::IBitOr.default_effects(), Some(span));
    let key_error = ctx.builder.create_named_block("nested.write.false.pending.type.error", Vec::new());
    branch(ctx, invalid, key_error, rethrow);
    ctx.builder.position_at_end(key_error);
    lower_pending_key_error(ctx, key, object, &pending_name, pending_slot, span);
    ctx.builder.position_at_end(rethrow);
    let pending = ctx.load_local(&pending_name, Some(span));
    ctx.clear_owned_hidden_temp(&pending_name, Some(span));
    ctx.emit_void(Op::PopCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(pending_slot)),
        Op::PopCallOperandOwner.default_effects(), Some(span));
    terminate_throw(ctx, pending.value);
}

/// Chains invalid array/object offsets above a deferred deprecation-handler exception.
fn lower_pending_key_error(
    ctx: &mut LoweringContext<'_, '_>, key: LoweredValue, object: LoweredValue,
    pending_name: &str, pending_slot: LocalSlotId, span: Span,
) {
    let object_name = ctx.builder.create_named_block("nested.write.key.object.name", Vec::new());
    let array_name = ctx.builder.create_named_block("nested.write.key.array.name", Vec::new());
    let named = ctx.builder.create_named_block("nested.write.key.name", vec![(IrType::Str, PhpType::Str)]);
    branch(ctx, object, object_name, array_name);
    ctx.builder.position_at_end(object_name);
    let target = RuntimeFnId::GetClass;
    let class = ctx.emit_value(Op::RuntimeCall, vec![key.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))), PhpType::Str,
        target.effects(), Some(span));
    ctx.builder.terminate(Terminator::Br { target: named, args: vec![class.value] });
    ctx.builder.position_at_end(array_name);
    let array = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral("array".into()), span));
    ctx.builder.terminate(Terminator::Br { target: named, args: vec![array.value] });
    ctx.builder.position_at_end(named);
    let name = ctx.builder.block_param(named, 0);
    let prefix = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral("Cannot access offset of type ".into()), span));
    let message = ctx.emit_value(Op::StrConcat, vec![prefix.value, name], None, PhpType::Str,
        Op::StrConcat.default_effects(), Some(span));
    // String edge arguments are detached copies, even when the predecessor lends metadata.
    crate::ir_lower::ownership::release_if_owned(ctx,
        LoweredValue { value: name, ir_type: IrType::Str }, Some(span));
    // These consecutive concats do not cross a user call or reset concat scratch.
    let suffix = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral(" on array".into()), span));
    let message = ctx.emit_value(Op::StrConcat, vec![message.value, suffix.value], None, PhpType::Str,
        Op::StrConcat.default_effects(), Some(span));
    let mut owners = Vec::new();
    let message = root_string(ctx, message, &mut owners, span);
    let message_name = ctx.declare_hidden_temp(PhpType::Str);
    let borrowed = crate::ir_lower::ownership::acquire_if_refcounted(ctx, message, Some(span));
    ctx.store_local(&message_name, borrowed, PhpType::Str, Some(span));
    let message_slot = ctx.local_slots[&message_name];
    ctx.emit_void(Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(message_slot)),
        Op::PushCallOperandOwner.default_effects(), Some(span));
    owners.push(message_slot);
    let escaped = lower_expr(ctx, &Expr::new(ExprKind::NewObject {
        class_name: crate::names::Name::unqualified("TypeError"),
        args: vec![Expr::new(ExprKind::Variable(message_name.clone()), span)],
    }, span));
    for owner in owners.into_iter().rev() {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
    }
    let pending = ctx.load_local(pending_name, Some(span));
    ctx.clear_owned_hidden_temp(pending_name, Some(span));
    ctx.emit_void(Op::PopCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(pending_slot)),
        Op::PopCallOperandOwner.default_effects(), Some(span));
    ctx.emit_void(Op::RuntimeCall, vec![escaped.value, pending.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionChain)), crate::ir::Effects::all(), Some(span));
    terminate_throw(ctx, escaped.value);
}

/// Where a string-offset rewrite publishes its owned result.
enum StringWriteTarget<'a> {
    /// A nested parent's Mixed cell (`$m["a"]["b"] = v` through a mixed chain).
    Cell(LoweredValue),
    /// A top-level local whose receiver an operand retyped to a string
    /// (`$a[0] = ($a = "abc") ? "Z" : "q"`).
    Local(&'a str),
}

/// Rebuilds a string offset on a local whose receiver an element write's operand retyped to a
/// string, storing the result back into that local.
pub(super) fn lower_string_write_to_local(
    ctx: &mut LoweringContext<'_, '_>, name: &str, parent: LoweredValue,
    key: LoweredValue, value: LoweredValue, span: Span,
) {
    lower_string_write(ctx, StringWriteTarget::Local(name), parent, key, value, span);
}

/// Rebuilds a single PHP string offset and publishes the owned result into its target.
fn lower_string_write(
    ctx: &mut LoweringContext<'_, '_>, target: StringWriteTarget<'_>, parent: LoweredValue,
    key: LoweredValue, value: LoweredValue, span: Span,
) {
    let mut owners = Vec::new();
    let source = cast_string(ctx, parent, span);
    let source = root_string(ctx, source, &mut owners, span);
    let offset = ctx.emit_value(Op::RuntimeCall, vec![key.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::StringWriteOffset)), PhpType::Int,
        effects_lookup::runtime_effects(), Some(span));
    let common_owners = owners.len();
    let length = ctx.emit_value(Op::StrLen, vec![source.value], None, PhpType::Int,
        Op::StrLen.default_effects(), Some(span));
    let zero = int(ctx, 0);
    let one = int(ctx, 1);
    let negative = compare(ctx, offset, zero, CmpPredicate::Slt, span);
    let from_end = binary(ctx, Op::IAdd, length, offset, span);
    let position = select_int(ctx, negative, from_end, offset);
    let invalid = compare(ctx, position, zero, CmpPredicate::Slt, span);
    let rejected = ctx.builder.create_named_block("string.write.negative", Vec::new());
    let writable = ctx.builder.create_named_block("string.write.valid", Vec::new());
    let done = ctx.builder.create_named_block("string.write.done", Vec::new());
    branch(ctx, invalid, rejected, writable);
    ctx.builder.position_at_end(rejected);
    let message = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral("Warning: Illegal string offset ".into()), span));
    let rendered = cast_string(ctx, offset, span);
    let message = ctx.emit_value(Op::StrConcat, vec![message.value, rendered.value], None, PhpType::Str,
        Op::StrConcat.default_effects(), Some(span));
    let newline = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral("\n".into()), span));
    let message = ctx.emit_value(Op::StrConcat, vec![message.value, newline.value], None, PhpType::Str,
        Op::StrConcat.default_effects(), Some(span));
    emit_diagnostic(ctx, message, span);
    ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    ctx.builder.position_at_end(writable);
    let rhs = cast_string(ctx, value, span);
    let rhs = root_string(ctx, rhs, &mut owners, span);
    let rhs_length = ctx.emit_value(Op::StrLen, vec![rhs.value], None, PhpType::Int,
        Op::StrLen.default_effects(), Some(span));
    let empty = compare(ctx, rhs_length, zero, CmpPredicate::Eq, span);
    let empty_error = ctx.builder.create_named_block("string.write.empty", Vec::new());
    let nonempty = ctx.builder.create_named_block("string.write.nonempty", Vec::new());
    branch(ctx, empty, empty_error, nonempty);
    ctx.builder.position_at_end(empty_error);
    lower_throw_access_error(ctx, "Cannot assign an empty string to a string offset", span);
    ctx.builder.position_at_end(nonempty);
    let multiple = compare(ctx, rhs_length, one, CmpPredicate::Sgt, span);
    let warn = ctx.builder.create_named_block("string.write.first.byte.warning", Vec::new());
    let copy = ctx.builder.create_named_block("string.write.copy", Vec::new());
    branch(ctx, multiple, warn, copy);
    ctx.builder.position_at_end(warn);
    diagnostic(ctx, "Warning: Only the first byte will be assigned to the string offset\n", span);
    ctx.builder.terminate(Terminator::Br { target: copy, args: Vec::new() });
    ctx.builder.position_at_end(copy);
    let prefix = string_call(ctx, RuntimeFnId::Substr, &[source, zero, position], &mut owners, span);
    let gap = binary(ctx, Op::ISub, position, length, span);
    let gap_positive = compare(ctx, gap, zero, CmpPredicate::Sgt, span);
    let gap = select_int(ctx, gap_positive, gap, zero);
    let space = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral(" ".into()), span));
    let padding = string_call(ctx, RuntimeFnId::StrRepeat, &[space, gap], &mut owners, span);
    let byte = string_call(ctx, RuntimeFnId::Substr, &[rhs, zero, one], &mut owners, span);
    let next = binary(ctx, Op::IAdd, position, one, span);
    let tail = binary(ctx, Op::ISub, length, next, span);
    let positive = compare(ctx, tail, zero, CmpPredicate::Sgt, span);
    let tail = select_int(ctx, positive, tail, zero);
    let suffix = string_call(ctx, RuntimeFnId::Substr, &[source, next, tail], &mut owners, span);
    let mut result = prefix;
    for part in [padding, byte, suffix] {
        let joined = ctx.emit_value(Op::StrConcat, vec![result.value, part.value], None, PhpType::Str,
            Op::StrConcat.default_effects(), Some(span));
        result = root_string(ctx, joined, &mut owners, span);
    }
    match target {
        StringWriteTarget::Cell(parent) => replace_cell(ctx, parent, result, span),
        StringWriteTarget::Local(name) => {
            ctx.store_local(name, result, PhpType::Str, Some(span));
        }
    }
    for owner in owners.drain(common_owners..).rev() {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
    }
    ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    ctx.builder.position_at_end(done);
    for owner in owners.into_iter().rev() {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
    }
}

/// Calls an existing typed string primitive and roots its result before another scratch writer.
fn string_call(ctx: &mut LoweringContext<'_, '_>, target: RuntimeFnId, args: &[LoweredValue],
    owners: &mut Vec<LocalSlotId>, span: Span) -> LoweredValue {
    let value = ctx.emit_value(Op::RuntimeCall, args.iter().map(|arg| arg.value).collect(),
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))), PhpType::Str,
        effects_lookup::runtime_effects(), Some(span));
    root_string(ctx, value, owners, span)
}

/// Copies scratch or borrowed bytes into an unwind-visible owner and consumes owned inputs.
fn root_string(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    owners: &mut Vec<LocalSlotId>, span: Span) -> LoweredValue {
    let persisted = ctx.emit_owned_value(Op::StrPersist, vec![value.value], None, PhpType::Str,
        Op::StrPersist.default_effects(), Some(span));
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    let (rooted, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, persisted, span);
    owners.extend(owner);
    rooted
}

/// Converts an already evaluated PHP value without reevaluating its source expression.
fn cast_string(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    ctx.emit_value(Op::Cast, vec![value.value], Some(Immediate::CastTarget(IrType::Str)), PhpType::Str,
        Op::Cast.default_effects(), Some(span))
}

/// Copies a replacement payload into an attached Mixed cell, retaining its persistent owner.
fn replace_cell(ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, value: LoweredValue, span: Span) {
    ctx.emit_void(Op::RuntimeCall, vec![parent.value, value.value], None,
        effects_lookup::runtime_effects(), Some(span));
}

/// Uses the unchanged native array/object setter for a non-string parent.
fn emit_set(ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue,
    key: LoweredValue, value: LoweredValue, span: Span, key_already_diagnosed: bool) {
    ctx.emit_void(Op::RuntimeCall, vec![parent.value, key.value, value.value],
        key_already_diagnosed.then_some(Immediate::Bool(true)),
        effects_lookup::runtime_effects(), Some(span));
}

/// Borrows a constant complete diagnostic and forwards it to PHP's shared warning channel.
fn diagnostic(ctx: &mut LoweringContext<'_, '_>, text: &str, span: Span) {
    let value = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral(text.into()), span));
    emit_diagnostic(ctx, value, span);
}

/// Preserves error-reporting masks, handler levels and catchable warning-handler throws.
fn emit_diagnostic(ctx: &mut LoweringContext<'_, '_>, message: LoweredValue, span: Span) {
    ctx.emit_void(Op::RuntimeCall, vec![message.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::WriteDiagnostic)),
        effects_lookup::runtime_effects(), Some(span));
}

/// Tests the selected parent's runtime tag without consuming its storage.
fn predicate(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    predicate: crate::ir::PhpTypePredicate, span: Span) -> LoweredValue {
    ctx.emit_value(Op::TypePredicate, vec![value.value], Some(Immediate::TypePredicate(predicate)),
        PhpType::Bool, Op::TypePredicate.default_effects(), Some(span))
}

/// Creates a nonowning integer literal for bounds and substring arguments.
fn int(ctx: &mut LoweringContext<'_, '_>, value: i64) -> LoweredValue {
    LoweredValue { value: ctx.builder.emit_const_i64(value), ir_type: IrType::I64 }
}

/// Emits a target-neutral integer comparison used by string-write bounds checks.
fn compare(ctx: &mut LoweringContext<'_, '_>, left: LoweredValue, right: LoweredValue,
    predicate: CmpPredicate, span: Span) -> LoweredValue {
    ctx.emit_value(Op::ICmp, vec![left.value, right.value], Some(Immediate::CmpPredicate(predicate)),
        PhpType::Bool, Op::ICmp.default_effects(), Some(span))
}

/// Emits a target-neutral integer calculation for offset rebasing and padding.
fn binary(ctx: &mut LoweringContext<'_, '_>, op: Op, left: LoweredValue,
    right: LoweredValue, span: Span) -> LoweredValue {
    ctx.emit_value(op, vec![left.value, right.value], None, PhpType::Int, op.default_effects(), Some(span))
}

/// Selects an integer with explicit CFG edges rather than target-specific condition registers.
fn select_int(ctx: &mut LoweringContext<'_, '_>, condition: LoweredValue,
    yes: LoweredValue, no: LoweredValue) -> LoweredValue {
    let selected = ctx.builder.create_named_block("string.write.select", vec![(IrType::I64, PhpType::Int)]);
    let when_true = ctx.builder.create_named_block("string.write.select.true", Vec::new());
    let when_false = ctx.builder.create_named_block("string.write.select.false", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value, then_target: when_true, then_args: Vec::new(),
        else_target: when_false, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(when_true);
    ctx.builder.terminate(Terminator::Br { target: selected, args: vec![yes.value] });
    ctx.builder.position_at_end(when_false);
    ctx.builder.terminate(Terminator::Br { target: selected, args: vec![no.value] });
    ctx.builder.position_at_end(selected);
    LoweredValue { value: ctx.builder.block_param(selected, 0), ir_type: IrType::I64 }
}

/// Splits the current EIR block without changing the selected operands' ownership.
fn branch(ctx: &mut LoweringContext<'_, '_>, condition: LoweredValue, yes: BlockId, no: BlockId) {
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value, then_target: yes, then_args: Vec::new(),
        else_target: no, else_args: Vec::new(),
    });
}
