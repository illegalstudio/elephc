//! Purpose:
//! Rejects non-object receivers before readonly initialization and setter checks.
//!
//! Called from:
//! - `super::instance_property_writes::lower_property_assign()` after receiver and RHS evaluation.
//!
//! Key details:
//! - Uses target-independent predicates and PHP's receiver-specific Error text.
//! - Error arms retire independent evaluated operands before throwing.

use super::*;
use crate::ir::PhpTypePredicate;

/// Checks boxed receivers without inspecting a nonexistent readonly property slot.
pub(super) fn guard_readonly_write_receiver(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, value: LoweredValue,
    property: &str, span: Span,
) {
    if ctx.builder.value_php_type(object.value).codegen_repr() != PhpType::Mixed {
        return;
    }
    let object_test = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let accepted = ctx.builder.create_named_block("readonly.receiver.object", Vec::new());
    let rejected = ctx.builder.create_named_block("readonly.receiver.invalid", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: object_test.value, then_target: accepted, then_args: Vec::new(),
        else_target: rejected, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(rejected);
    for (predicate, name) in [
        (None, "null"), (Some(PhpTypePredicate::Bool), "bool"),
        (Some(PhpTypePredicate::Int), "int"), (Some(PhpTypePredicate::Float), "float"),
        (Some(PhpTypePredicate::String), "string"), (Some(PhpTypePredicate::Array), "array"),
        (Some(PhpTypePredicate::Resource), "resource"),
    ] {
        let op = if predicate.is_some() { Op::TypePredicate } else { Op::IsNull };
        let test = ctx.emit_value(op, vec![object.value], predicate.map(Immediate::TypePredicate),
            PhpType::Bool, op.default_effects(), Some(span));
        let matched = ctx.builder.create_named_block("readonly.receiver.type", Vec::new());
        let next = ctx.builder.create_named_block("readonly.receiver.next", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: test.value, then_target: matched, then_args: Vec::new(),
            else_target: next, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(matched);
        if name == "bool" {
            let truthy = ctx.emit_value(Op::IsTruthy, vec![object.value], None, PhpType::Bool,
                Op::IsTruthy.default_effects(), Some(span));
            let true_arm = ctx.builder.create_named_block("readonly.receiver.true", Vec::new());
            let false_arm = ctx.builder.create_named_block("readonly.receiver.false", Vec::new());
            ctx.builder.terminate(Terminator::CondBr {
                cond: truthy.value, then_target: true_arm, then_args: Vec::new(),
                else_target: false_arm, else_args: Vec::new(),
            });
            ctx.builder.position_at_end(true_arm);
            reject_receiver(ctx, object, value, property, "true", span);
            ctx.builder.position_at_end(false_arm);
            reject_receiver(ctx, object, value, property, "false", span);
        } else {
            reject_receiver(ctx, object, value, property, name, span);
        }
        ctx.builder.position_at_end(next);
    }
    reject_receiver(ctx, object, value, property, "unknown", span);
    ctx.builder.position_at_end(accepted);
}

/// Retires evaluated owners and raises the ordinary assignment Error for the invalid receiver.
fn reject_receiver(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, value: LoweredValue,
    property: &str, receiver_type: &str, span: Span,
) {
    super::instance_property_writes::lower_readonly_write_error(ctx, object, value,
        &format!("Attempt to assign property \"{property}\" on {receiver_type}"), span);
}
