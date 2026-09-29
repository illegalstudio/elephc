//! Purpose:
//! Lowers the expression form of a string offset write, `($s[$i] = $v)`, whose value is the
//! byte PHP stored rather than `$v`.
//!
//! Called from:
//! - `crate::ir_lower::expr::assignments::lower_assignment_expr()`.
//!
//! Key details:
//! - PHP evaluates the expression to the first byte of the value, or to `null` when the offset
//!   lies before the start of the string and nothing was written; the result is therefore a
//!   `?string`, held in a boxed merge temporary.
//! - The index and value are lowered exactly once per runtime path. The parser has already
//!   bound a value with side effects to a prelude temporary, so neither branch below can
//!   repeat a call.
//! - A boxed (`mixed`) local is a string offset write only when it holds a string at run
//!   time. The branch tests the tag first: the string arm performs the string write, the other
//!   arm the ordinary array write whose value is the assigned value itself.
//! - Known divergence: when a warning handler replaces the variable mid-write, php abandons
//!   the write and evaluates to `null`. The boxed writer abandons it too, but the expression
//!   still evaluates to the byte; a concrete `string` local (reachable from a handler only as
//!   a by-reference parameter) also keeps the written string.

use super::*;

/// Lowers `($name[$index] = $value)` when `$name` is a local that holds or may hold a string,
/// and returns `None` for every other target so the generic assignment lowering runs.
///
/// A concrete `string` local always takes the string write. A boxed local that may hold a
/// string branches on its runtime tag, which needs `result_target`, the replayable value
/// the parser bound for the array-write arm.
pub(super) fn lower_string_offset_assignment_expr(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    result_target: Option<&Expr>,
    span: Span,
    key_already_diagnosed: bool,
) -> Option<LoweredValue> {
    let ExprKind::ArrayAccess { array, index } = &target.kind else {
        return None;
    };
    let ExprKind::Variable(name) = &array.kind else {
        return None;
    };
    if crate::ir_lower::stmt::local_is_string_offset_target(ctx, name) {
        return Some(lower_known_string_offset_assign_expr(ctx, name, index, value, span));
    }
    let result_target = result_target?;
    if !crate::ir_lower::stmt::local_may_hold_boxed_string(ctx, name) {
        return None;
    }
    Some(lower_boxed_offset_assign_expr(
        ctx,
        name,
        index,
        target,
        value,
        result_target,
        span,
        key_already_diagnosed,
    ))
}

/// The PHP type of a string offset assignment expression: the written byte or `null`.
fn string_offset_result_type() -> PhpType {
    PhpType::Union(vec![PhpType::Str, PhpType::Void])
}

/// Lowers the expression form on a concrete `string` local.
fn lower_known_string_offset_assign_expr(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) -> LoweredValue {
    let result_type = string_offset_result_type();
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let merge = ctx.builder.create_named_block("str.offset.assign.merge", Vec::new());
    let written = crate::ir_lower::stmt::lower_string_offset_write(ctx, name, index, value, span, true)
        .expect("a string offset write asked for its result reports it");
    let initialized = store_string_offset_result(ctx, &temp_name, &result_type, written, merge, span);
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(initialized);
    take_owned_temp(ctx, &temp_name, span)
}

/// Lowers the expression form on a boxed local that may hold a string at run time.
///
/// The string arm returns PHP's `?string` result; the other arm performs the ordinary write
/// and returns `result_target`, the assigned value. Both land in one boxed merge temporary.
#[allow(clippy::too_many_arguments)]
fn lower_boxed_offset_assign_expr(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    index: &Expr,
    target: &Expr,
    value: &Expr,
    result_target: &Expr,
    span: Span,
    key_already_diagnosed: bool,
) -> LoweredValue {
    let result_type = PhpType::Mixed;
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let split_initialized = ctx.initialized_slots_snapshot();
    let string_block = ctx.builder.create_named_block("str.offset.assign.string", Vec::new());
    let other_block = ctx.builder.create_named_block("str.offset.assign.other", Vec::new());
    let merge = ctx.builder.create_named_block("str.offset.assign.merge", Vec::new());
    let subject = ctx.load_local(name, Some(span));
    ctx.builder.set_value_ownership(subject.value, Ownership::Borrowed);
    let is_string = ctx.emit_value(
        Op::TypePredicate,
        vec![subject.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::String)),
        PhpType::Bool,
        Op::TypePredicate.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_string.value,
        then_target: string_block,
        then_args: Vec::new(),
        else_target: other_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(string_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    let written = crate::ir_lower::stmt::lower_boxed_string_offset_write(ctx, name, index, value, span);
    // Both arms of the result store reach the merge, so this arm always does.
    let string_initialized =
        store_string_offset_result(ctx, &temp_name, &result_type, written, merge, span);

    ctx.builder.position_at_end(other_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    lower_non_local_assignment_write_with_diagnosed_key(ctx, target, value, span, key_already_diagnosed);
    store_expr_into_temp(ctx, &temp_name, result_type, result_target, span);
    let other_reachable = !ctx.builder.insertion_block_is_terminated();
    let other_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(
        &split_initialized,
        string_initialized,
        true,
        other_initialized,
        other_reachable,
    ));
    take_owned_temp(ctx, &temp_name, span)
}

/// Stores a string offset write's result into `temp_name` and branches to `merge`.
///
/// The written byte when the offset reached the string, otherwise `null` (the unused byte is
/// released on that arm). Both arms reach `merge`; the builder is left in a terminated block,
/// and the returned set is the initialized slots on arrival at `merge`.
fn store_string_offset_result(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    result_type: &PhpType,
    written: crate::ir_lower::stmt::StringOffsetWriteResult,
    merge: BlockId,
    span: Span,
) -> HashSet<LocalSlotId> {
    let split_initialized = ctx.initialized_slots_snapshot();
    let wrote_block = ctx.builder.create_named_block("str.offset.assign.wrote", Vec::new());
    let illegal_block = ctx.builder.create_named_block("str.offset.assign.illegal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: written.wrote.value,
        then_target: wrote_block,
        then_args: Vec::new(),
        else_target: illegal_block,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(wrote_block);
    store_value_into_temp(ctx, temp_name, result_type.clone(), written.byte, span);
    let initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);
    ctx.builder.position_at_end(illegal_block);
    ctx.restore_initialized_slots(split_initialized);
    crate::ir_lower::ownership::release_if_owned(ctx, written.byte, Some(span));
    let null_value = ctx.builder.emit_const_null();
    let null_lowered = LoweredValue { value: null_value, ir_type: IrType::I64 };
    store_value_into_temp(ctx, temp_name, result_type.clone(), null_lowered, span);
    branch_to(ctx, merge);
    initialized
}
