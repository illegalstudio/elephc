//! Purpose:
//! Preserves deferred trait property-default errors at class initialization boundaries.
//!
//! Called from:
//! - Object construction, static-property operations, and by-name initialization thunks.
//!
//! Key details:
//! - Metadata-only class operations must not evaluate property defaults.
//! - Constructor-promoted defaults are evaluated separately when their argument is omitted.

use crate::span::Span;
use crate::parser::ast::StaticReceiver;
use crate::ir::{CmpPredicate, Immediate, Op, Ownership, PhpTypePredicate, RuntimeCallTarget, RuntimeFnId, Terminator};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;
use super::context::{LoweredValue, LoweringContext};

/// Raises the recorded initialization error without allocating the invalid class.
pub(super) fn for_class(
    ctx: &mut LoweringContext<'_, '_>, class_name: &str, span: Span,
) -> Option<LoweredValue> {
    let message = ctx.classes.get(class_name.trim_start_matches('\\'))
        .and_then(|class| class.deferred_property_default_error.clone())?;
    Some(super::stmt::lower_throw_access_error_expr(ctx, &message, span))
}

/// Applies the class-wide check even when the accessed static property has a valid default.
pub(super) fn for_receiver(
    ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, span: Span,
) -> Option<LoweredValue> {
    let name = super::expr::static_receiver_class_name(ctx, receiver)?;
    for_class(ctx, &name, span)
}

/// Checks a runtime class name without creating an owning dispatch temporary.
pub(super) fn for_dynamic_class(ctx: &mut LoweringContext<'_, '_>, name: LoweredValue, span: Span) {
    if !ctx.classes.values().any(|class| class.deferred_property_default_error.is_some()) { return; }
    let ty = ctx.builder.value_php_type(name.value).codegen_repr();
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        let string = ctx.emit_value(Op::TypePredicate, vec![name.value],
            Some(Immediate::TypePredicate(PhpTypePredicate::String)), PhpType::Bool,
            Op::TypePredicate.default_effects(), Some(span));
        let check = ctx.builder.create_named_block("class_default.string", Vec::new());
        let done = ctx.builder.create_named_block("class_default.nonstring", Vec::new());
        ctx.builder.terminate(Terminator::CondBr { cond: string.value,
            then_target: check, then_args: Vec::new(), else_target: done, else_args: Vec::new() });
        ctx.builder.position_at_end(check);
        let unboxed = ctx.emit_value(Op::MixedUnbox, vec![name.value], None, PhpType::Str,
            Op::MixedUnbox.default_effects(), Some(span));
        ctx.builder.set_value_ownership(unboxed.value, Ownership::Borrowed);
        for_dynamic_class(ctx, unboxed, span);
        ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
        ctx.builder.position_at_end(done);
        return;
    }
    if ty != PhpType::Str { return; }
    let mut classes: Vec<_> = ctx.classes.iter()
        .filter(|(_, class)| class.deferred_property_default_error.is_some())
        .map(|(name, _)| name.clone()).collect();
    classes.sort();
    for class in classes {
        let literal = super::expr::lower_expr(ctx, &Expr::new(ExprKind::StringLiteral(class.clone()), span));
        let comparison = ctx.emit_value(Op::RuntimeCall, vec![name.value, literal.value],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::Strcasecmp))),
            PhpType::Int, RuntimeFnId::Strcasecmp.effects(), Some(span));
        let zero = super::expr::lower_expr(ctx, &Expr::new(ExprKind::IntLiteral(0), span));
        let matches = ctx.emit_value(Op::ICmp, vec![comparison.value, zero.value],
            Some(Immediate::CmpPredicate(CmpPredicate::Eq)), PhpType::Bool,
            Op::ICmp.default_effects(), Some(span));
        let error = ctx.builder.create_named_block("class_default.error", Vec::new());
        let next = ctx.builder.create_named_block("class_default.next", Vec::new());
        ctx.builder.terminate(Terminator::CondBr { cond: matches.value,
            then_target: error, then_args: Vec::new(), else_target: next, else_args: Vec::new() });
        ctx.builder.position_at_end(error);
        for_class(ctx, &class, span);
        ctx.builder.terminate(Terminator::Br { target: next, args: Vec::new() });
        ctx.builder.position_at_end(next);
    }
}
