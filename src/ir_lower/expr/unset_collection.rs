//! Purpose:
//! Chooses cycle-collection safe points after array element removal.
//!
//! Called from:
//! - `super::unset::lower_unset_locals()`.
//!
//! Key details:
//! - A removal flag gates collection only when key evaluation cannot change the owner graph.
//! - Root, property, object-offset and effectful-key unsets retain unconditional collection.

use super::*;

/// Accepts scalar key reads whose evaluation and normalization cannot invoke callbacks.
pub(super) fn key_has_no_callbacks(ctx: &LoweringContext<'_, '_>, key: &Expr) -> bool {
    match &key.kind {
        ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_) | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Variable(name) => matches!(
            ctx.local_type(name).codegen_repr(), PhpType::Int | PhpType::Str | PhpType::Bool
        ),
        ExprKind::FunctionCall { name, args } if args.len() == 1
            && !ctx.functions.contains_key(name.as_str())
            && !ctx.extern_functions.contains_key(name.as_str()) => {
            let Some(def) = crate::builtins::registry::lookup(name.as_str().trim_start_matches('\\'))
                else { return false; };
            matches!(def.spec.semantics.lowering,
                crate::builtins::semantics::BuiltinLowering::Runtime(
                    crate::ir::RuntimeCallTarget::UnaryString(_)))
                && matches!(def.spec.semantics.effects,
                    crate::builtins::semantics::BuiltinEffects::Static(effects) if effects.is_pure())
                && def.return_type == PhpType::Str
                && string_source_has_no_callbacks(ctx, &args[0])
        }
        _ => false,
    }
}

/// Requires a concrete string before a pure transform so weak coercion cannot invoke PHP.
fn string_source_has_no_callbacks(ctx: &LoweringContext<'_, '_>, source: &Expr) -> bool {
    match &source.kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::Variable(name) => ctx.local_type(name).codegen_repr() == PhpType::Str,
        ExprKind::FunctionCall { .. } => key_has_no_callbacks(ctx, source),
        _ => false,
    }
}

/// Combines removal flags without short-circuiting later unset targets or key expressions.
pub(super) fn combine_removals(
    ctx: &mut LoweringContext<'_, '_>,
    previous: Option<LoweredValue>,
    removed: LoweredValue,
    span: Span,
) -> LoweredValue {
    let Some(previous) = previous else { return removed; };
    ctx.emit_value(Op::IBitOr, vec![previous.value, removed.value], None,
        PhpType::Bool, Op::IBitOr.default_effects(), Some(span))
}

/// Keeps one final safe point for actual removals or conservatively effectful unset lists.
pub(super) fn collect_after_unset(
    ctx: &mut LoweringContext<'_, '_>,
    changed: Option<LoweredValue>,
    unconditional: bool,
    span: Span,
) {
    if unconditional || changed.is_none() {
        crate::ir_lower::ownership::collect_cycles(ctx, Some(span));
        return;
    }
    let collect = ctx.builder.create_named_block("unset.collect", Vec::new());
    let merge = ctx.builder.create_named_block("unset.done", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: changed.unwrap().value,
        then_target: collect, then_args: Vec::new(),
        else_target: merge, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(collect);
    crate::ir_lower::ownership::collect_cycles(ctx, Some(span));
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
}
