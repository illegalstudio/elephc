//! Purpose:
//! Shares boxed offset-write parent checks and deferred false-conversion completion.
//!
//! Called from:
//! - Boxed writes after receiver, key and value owners have been published.
//!
//! Key details:
//! - Existing false-parent helpers preserve warning order and chained callback exceptions.
//! - Ordinary array-key guards inspect the selected parent after autovivification.
//! - Non-boolean parents keep the normal string, ArrayAccess and array dispatch.

use super::*;

/// Rejects scalar parents and completes false conversion using the existing nested-write rules.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue,
    key: LoweredValue, value: LoweredValue, span: Span, key_already_diagnosed: bool,
) {
    if !super::nested_write_guards::guard_scalar_write_parent(ctx, parent, span) { return; }
    if ctx.builder.value_php_type(parent.value).codegen_repr() != PhpType::Mixed {
        store(ctx, parent, key, value, span, key_already_diagnosed);
        return;
    }
    let boolean = ctx.emit_value(Op::TypePredicate, vec![parent.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Bool)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let convert = ctx.builder.create_named_block("offset.false.convert", Vec::new());
    let ordinary = ctx.builder.create_named_block("offset.parent.store", Vec::new());
    let done = ctx.builder.create_named_block("offset.parent.done", Vec::new());
    let initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr {
        cond: boolean.value, then_target: convert, then_args: Vec::new(),
        else_target: ordinary, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(convert);
    let borrowed_value = crate::ir_lower::ownership::borrow_value(ctx, value, Some(span));
    ctx.with_independent_write_receiver(|ctx| {
        let pending = super::nested_false_errors::capture(ctx, parent, span);
        super::nested_false_errors::handle_pending(ctx, pending, Some(key), span, |ctx| {
            store(ctx, parent, key, borrowed_value, span, key_already_diagnosed);
        });
        store(ctx, parent, key, borrowed_value, span, key_already_diagnosed);
    });
    let converted = ctx.initialized_slots_snapshot();
    branch_to(ctx, done);
    ctx.builder.position_at_end(ordinary);
    ctx.restore_initialized_slots(initialized);
    store(ctx, parent, key, value, span, key_already_diagnosed);
    let mut joined = ctx.initialized_slots_snapshot();
    joined.retain(|slot| converted.contains(slot));
    branch_to(ctx, done);
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(joined);
}

/// Applies the selected key policy before delegating to the existing receiver dispatcher.
fn store(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue,
    key: LoweredValue, value: LoweredValue, span: Span, key_already_diagnosed: bool,
) {
    reject_invalid_array_key(ctx, parent, key, span);
    crate::ir_lower::array_key_guards::for_prepared_write(ctx, parent, key, span);
    super::string_offset_writes::lower_boxed_set(ctx, parent, key, value, span, key_already_diagnosed);
}

/// Rejects array/object keys only when this access selects a PHP array, preserving object setters.
fn reject_invalid_array_key(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, key: LoweredValue, span: Span,
) {
    if !matches!(ctx.builder.value_php_type(key.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_) | PhpType::Array(_) | PhpType::AssocArray { .. }
        | PhpType::Object(_) | PhpType::Callable | PhpType::Iterable)
    { return; }
    let array = crate::ir_lower::semantic_guards::matches_any_kind(ctx, parent,
        &[crate::ir::PhpTypePredicate::Array], Some(span));
    let invalid = crate::ir_lower::semantic_guards::matches_any_kind(ctx, key,
        &[crate::ir::PhpTypePredicate::Array, crate::ir::PhpTypePredicate::Object], Some(span));
    let rejected = ctx.emit_value(Op::IBitAnd, vec![array.value, invalid.value], None,
        PhpType::Bool, Op::IBitAnd.default_effects(), Some(span));
    let error = ctx.builder.create_named_block("offset.key.type.error", Vec::new());
    let accepted = ctx.builder.create_named_block("offset.key.type.accepted", Vec::new());
    let initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr {
        cond: rejected.value, then_target: error, then_args: Vec::new(),
        else_target: accepted, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(error);
    super::nested_false_errors::raise_invalid_key_type_error(ctx, key, span);
    ctx.builder.position_at_end(accepted);
    ctx.restore_initialized_slots(initialized);
}
