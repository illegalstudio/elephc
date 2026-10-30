//! Purpose:
//! Bound compiler-generated statement write captures across callbacks and unwinding.
//!
//! Called from:
//! - Assignment stores and discarded expression or synthetic statement lowering.
//!
//! Key details:
//! - Only the dedicated parser prefix participates; expression and static captures keep their paths.
//! - Owners register after stores, using current slots, and retire in reverse after the result.

use super::context::{LoweredValue, LoweringContext};
use crate::ir::{Immediate, IrType, LocalSlotId, Op, Ownership};
use crate::types::PhpType;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind, SCOPED_WRITE_TEMP_PREFIX};
use crate::span::Span;

/// One statement's live slots, cloned with speculative lowering state.
#[derive(Clone)]
pub(super) struct ScopedWriteScope {
    owners: Vec<LocalSlotId>,
    excluded: Option<String>,
    preserve: bool,
}

/// Identifies an unforgeable capture created only by the scoped statement planners.
fn scoped_name(name: &str) -> bool {
    name.starts_with(SCOPED_WRITE_TEMP_PREFIX) && crate::names::is_generated_local_name(name)
}

/// Opens a scope only for a discarded assignment's own direct capture prelude.
pub(super) fn begin_expression(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> Option<usize> {
    let ExprKind::Assignment { prelude, conditional_value_temp, .. } = &expr.kind else { return None; };
    if !prelude.iter().any(|stmt| matches!(&stmt.kind, StmtKind::Assign { name, .. } if scoped_name(name))) {
        return None;
    }
    Some(begin(ctx, conditional_value_temp.clone(), true))
}

/// Recognizes newly scoped plain/append groups without claiming legacy synthetic protocols.
pub(super) fn begin_group(ctx: &mut LoweringContext<'_, '_>, body: &[Stmt]) -> Option<usize> {
    if !body.iter().any(|stmt| matches!(&stmt.kind, StmtKind::Assign { name, .. } if scoped_name(name))) {
        return None;
    }
    Some(begin(ctx, None, group_reuses_write_target(body)))
}

/// Keeps computed receiver and key captures available for both halves of an indexed update.
fn group_reuses_write_target(body: &[Stmt]) -> bool {
    let Some(stmt) = body.last() else { return false; };
    match &stmt.kind {
        StmtKind::PropertyArrayAssign { object, property, index, value } =>
            super::stmt::desugared_element_update(value, stmt.span, |read| {
                matches!(&read.kind, ExprKind::ArrayAccess { array, index: read_index }
                    if read_index.as_ref() == index
                        && matches!(&array.kind, ExprKind::PropertyAccess { object: read_object, property: read_property }
                            if read_object == object && read_property == property))
            }).is_some(),
        StmtKind::NestedArrayAssign { target, value } =>
            super::stmt::desugared_element_update(value, stmt.span, |read| read == target).is_some(),
        _ => false,
    }
}

/// Records a stack boundary while the conditional probe keeps its existing independent owner.
fn begin(ctx: &mut LoweringContext<'_, '_>, excluded: Option<String>, preserve: bool) -> usize {
    let boundary = ctx.scoped_write_scopes.len();
    ctx.scoped_write_scopes.push(ScopedWriteScope { owners: Vec::new(), excluded, preserve });
    boundary
}

/// Publishes the actual slot after a scoped assignment has finished storing and retyping it.
pub(super) fn register_store(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) {
    if !scoped_name(name) || ctx.builder.insertion_block_is_terminated() { return; }
    let scope = ctx.scoped_write_scopes.last().expect("scoped write capture has a statement boundary");
    if scope.excluded.as_deref() == Some(name) { return; }
    let ty = ctx.local_type(name);
    if !Ownership::php_type_needs_lifetime_tracking(&ty)
        || matches!(ty.codegen_repr(), crate::types::PhpType::Buffer(_)) { return; }
    let slot = ctx.local_slots[name];
    if scope.owners.contains(&slot) { return; }
    super::expr::register_owned_call_operand(ctx, slot, span);
    ctx.scoped_write_scopes.last_mut().unwrap().owners.push(slot);
}

/// Retires captures after a discarded result, or leaves terminated paths to runtime unwinding.
pub(super) fn finish(ctx: &mut LoweringContext<'_, '_>, boundary: Option<usize>, span: Span) {
    let Some(boundary) = boundary else { return; };
    assert_eq!(ctx.scoped_write_scopes.len(), boundary + 1, "nested statement write scopes must balance");
    let scope = ctx.scoped_write_scopes.pop().unwrap();
    if ctx.builder.insertion_block_is_terminated() { return; }
    for slot in scope.owners.into_iter().rev() {
        super::expr::retire_owned_call_operand(ctx, slot, span);
    }
}

/// Selects only PHP heap values; scalar captures keep ordinary unboxed storage.
fn tracked_capture(name: &str, ty: &PhpType) -> bool {
    scoped_name(name) && Ownership::php_type_needs_lifetime_tracking(ty)
        && !matches!(ty.codegen_repr(), PhpType::Buffer(_) | PhpType::Pointer(_) | PhpType::Packed(_))
}

/// Consumes a one-use captured operand like its original inline temporary.
pub(super) fn take_value(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) -> Option<LoweredValue> {
    if !scoped_name(name) { return None; }
    let preserve = ctx.scoped_write_scopes.last()?.preserve;
    let ty = ctx.local_type(name);
    if !tracked_capture(name, &ty) { return None; }
    let value = ctx.load_local(name, Some(span));
    ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
    if preserve { return Some(value); }
    let owned = super::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.builder.set_value_ownership(owned.value, Ownership::Owned);
    let slot = ctx.local_slots[name];
    ctx.emit_void(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(), Some(span));
    Some(owned)
}

/// Converts a single-use name while its source and converted result have unwind guards.
///
/// Coalescing retains its original name for the second conversion. Other statements
/// return the staged string owner without releasing it, exactly like an inline name.
pub(super) fn property_name(ctx: &mut LoweringContext<'_, '_>, expr: &Expr, span: Span) -> Option<LoweredValue> {
    let ExprKind::Variable(name) = &expr.kind else { return None; };
    if !scoped_name(name) || ctx.scoped_write_scopes.last()?.excluded.is_some()
        || !tracked_capture(name, &ctx.local_type(name)) { return None; }
    let slot = ctx.local_slots[name];
    let raw = ctx.load_local(name, Some(span));
    ctx.builder.set_value_ownership(raw.value, Ownership::Borrowed);
    let converted = if ctx.builder.value_php_type(raw.value).codegen_repr() == PhpType::Str { raw } else {
        ctx.emit_value(Op::Cast, vec![raw.value], Some(Immediate::CastTarget(IrType::Str)),
            PhpType::Str, Op::Cast.default_effects(), Some(span))
    };
    let temp = ctx.declare_owned_hidden_temp(PhpType::Str);
    super::expr::store_value_into_temp(ctx, &temp, PhpType::Str, converted, span);
    let guard = ctx.local_slots[&temp];
    super::expr::register_owned_call_operand(ctx, guard, span);
    ctx.emit_void(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(), Some(span));
    super::expr::unregister_owned_call_operand(ctx, guard, span);
    let owned = super::expr::take_owned_temp(ctx, &temp, span);
    ctx.builder.set_value_ownership(owned.value, Ownership::Owned);
    Some(owned)
}

/// Enables one-use transfers once a computed update has claimed one receiver snapshot.
pub(super) fn consume_update_operands(ctx: &mut LoweringContext<'_, '_>) {
    ctx.scoped_write_scopes.last_mut().expect("scoped update has a statement boundary").preserve = false;
}
