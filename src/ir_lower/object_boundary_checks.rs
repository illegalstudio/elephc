//! Purpose:
//! Emits PHP's runtime class check for a boxed `mixed` or wider-object value that reaches a
//! declared class, interface, `object` or nullable-class boundary.
//!
//! Called from:
//! - Declared return coercion (`object_boundaries::guard_runtime_object_return`).
//! - Call-argument coercion for class-like parameters.
//!
//! Key details:
//! - The accept set comes from `crate::types::object_boundary`, the predicate the checker used
//!   to admit the value, so every admitted boundary is guarded.
//! - `static` is tested against the called class, the way PHP checks a late-static return.
//! - The value stays boxed; a caller whose declared storage is a raw object unboxes it on the
//!   accepted edge.
//! - The rejected edge parks the value in a rooted hidden slot so the message can name its
//!   runtime type once without re-evaluating the source expression.
//! - A bare `object` boundary unboxes to a raw object pointer, which cannot hold elephc's
//!   closure storage, so a `Closure` is rejected there (PHP accepts it).

use crate::ir::{Immediate, IrHeapKind, IrType, Op, PhpTypePredicate, Terminator};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::span::Span;
use crate::types::object_boundary::ObjectBoundary;
use crate::types::PhpType;

/// Branches to the accepted edge when `value` satisfies `boundary`, and otherwise throws
/// `TypeError("<prefix><runtime type><suffix>")`.
///
/// `release_on_reject` releases `value` before the throw, for callers whose own cleanup only
/// runs on the accepted edge. Returns `value` unchanged, positioned on the
/// accepted edge. A value that is not boxed is returned without a check.
pub(crate) fn guard_runtime_object_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    boundary: &ObjectBoundary,
    prefix: Expr,
    suffix: &str,
    release_on_reject: bool,
    span: Span,
) -> LoweredValue {
    if !matches!(value.ir_type, IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)) {
        return value;
    }
    let initialized = ctx.initialized_slots_snapshot();
    let accepted = ctx.builder.create_named_block("object.boundary.runtime.accepted", vec![]);
    let rejected = ctx.builder.create_named_block("object.boundary.runtime.rejected", vec![]);
    if raw_object_storage(boundary) {
        reject_closure_payload(ctx, value, rejected, span);
    }
    let mut tests = Vec::new();
    if boundary.nullable {
        tests.push(None);
    }
    tests.extend(boundary.classes.iter().map(Some));
    for class in tests {
        let condition = match class {
            None => ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
                Op::IsNull.default_effects(), Some(span)),
            Some(class) if class.is_empty() || class.eq_ignore_ascii_case("object") => ctx.emit_value(
                Op::TypePredicate, vec![value.value],
                Some(Immediate::TypePredicate(PhpTypePredicate::Object)), PhpType::Bool,
                Op::TypePredicate.default_effects(), Some(span)),
            Some(class) if class.eq_ignore_ascii_case("static") => late_static_instance_of(ctx, value, span),
            Some(class) => {
                let data = ctx.intern_class_name(class);
                ctx.emit_value(Op::InstanceOf, vec![value.value], Some(Immediate::Data(data)),
                    PhpType::Bool, Op::InstanceOf.default_effects(), Some(span))
            }
        };
        let next = ctx.builder.create_named_block("object.boundary.runtime.next", vec![]);
        ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
            then_target: accepted, then_args: vec![], else_target: next, else_args: vec![] });
        ctx.builder.position_at_end(next);
    }
    ctx.builder.terminate(Terminator::Br { target: rejected, args: vec![] });
    ctx.builder.position_at_end(rejected);
    throw_object_boundary_error(ctx, value, prefix, suffix, release_on_reject, span);
    ctx.builder.position_at_end(accepted);
    ctx.restore_initialized_slots(initialized);
    value
}

/// Returns whether the boundary is a bare, non-nullable `object`, whose declared storage is a
/// raw object pointer.
fn raw_object_storage(boundary: &ObjectBoundary) -> bool {
    !boundary.nullable && boundary.classes.len() == 1
        && (boundary.classes[0].is_empty() || boundary.classes[0].eq_ignore_ascii_case("object"))
}

/// Sends a closure payload to the rejected edge.
///
/// PHP accepts a `Closure` at an `object` boundary, but elephc keeps closures in callable
/// storage that a raw object pointer cannot hold: unboxing one would hand the callee a
/// descriptor read as an object. Throwing the boundary `TypeError` keeps that value out.
fn reject_closure_payload(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, rejected: crate::ir::BlockId, span: Span,
) {
    let data = ctx.intern_class_name("Closure");
    let closure = ctx.emit_value(Op::InstanceOf, vec![value.value], Some(Immediate::Data(data)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
    let next = ctx.builder.create_named_block("object.boundary.runtime.not_closure", vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: closure.value,
        then_target: rejected, then_args: vec![], else_target: next, else_args: vec![] });
    ctx.builder.position_at_end(next);
}

/// Tests `value instanceof static` against the called class: `$this` in an instance method,
/// `static::class` in a static one.
fn late_static_instance_of(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span,
) -> LoweredValue {
    let target = if ctx.local_slots.contains_key("this") {
        ctx.load_local("this", Some(span))
    } else {
        crate::ir_lower::expr::lower_expr(ctx, &Expr::new(ExprKind::ClassConstant {
            receiver: crate::parser::ast::StaticReceiver::Static,
        }, span))
    };
    let result = ctx.emit_value(Op::InstanceOfDynamic, vec![value.value, target.value], None,
        PhpType::Bool, Op::InstanceOfDynamic.default_effects(), Some(span));
    if ctx.value_needs_release_after_use(target) {
        crate::ir_lower::ownership::release_if_owned(ctx, target, Some(span));
    }
    result
}

/// Throws the boundary `TypeError`, naming the rejected value's `get_debug_type()` spelling.
fn throw_object_boundary_error(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    prefix: Expr,
    suffix: &str,
    release_on_reject: bool,
    span: Span,
) {
    let temp = ctx.declare_hidden_temp(PhpType::Mixed);
    let rooted = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.store_local(&temp, rooted, PhpType::Mixed, Some(span));
    let slot = ctx.local_slots[&temp];
    ctx.emit_void(Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PushCallOperandOwner.default_effects(), Some(span));
    if release_on_reject {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    let given = crate::ir_lower::expr::enum_argument_guards::runtime_debug_type(&Expr::var(&temp), span);
    let message = Expr::binop(Expr::binop(prefix, BinOp::Concat, given), BinOp::Concat,
        Expr::string_lit(suffix));
    let error = crate::ir_lower::expr::lower_expr(ctx, &Expr::new(ExprKind::NewObject {
        class_name: crate::names::Name::unqualified("TypeError"), args: vec![message],
    }, span));
    crate::ir_lower::expr::retire_owned_call_operand(ctx, slot, span);
    crate::ir_lower::stmt::throw_lowered_exception(ctx, error.value);
}
