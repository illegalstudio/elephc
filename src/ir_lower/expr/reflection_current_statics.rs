//! Purpose:
//! Builds Reflection's current static-property map when a metadata accessor reads it.
//!
//! Called from:
//! - Builtin ReflectionClass, ReflectionObject and ReflectionEnum private-field reads.
//!
//! Key details:
//! - Constructing a reflector owns declaration metadata without retaining live static values.
//! - Runtime class selection stays in EIR and only the selected map is materialized.

use super::*;

/// Replaces the builtin accessor's private cache read with an independent current-value map.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, op: Op, expr: &Expr,
) -> Option<LoweredValue> {
    if property != "__static_properties" || op != Op::PropGet
        || !ctx.current_class.as_deref().is_some_and(|class|
            matches!(class, "ReflectionClass" | "ReflectionObject" | "ReflectionEnum")
                && ctx.classes.get(class).is_some_and(|info| info.is_internal))
    { return None; }
    let name = lower_property_get_from_value(ctx, object, "__name", Op::PropGet, expr);
    let mut candidates = ctx.classes.iter().filter(|(_, info)| !info.static_properties.is_empty())
        .map(|(name, _)| name.clone()).collect::<Vec<_>>();
    candidates.sort();
    let result_type = PhpType::AssocArray { key: Box::new(PhpType::Str), value: Box::new(PhpType::Mixed) };
    let result = ctx.declare_owned_hidden_temp(result_type.clone());
    let merge = ctx.builder.create_named_block("reflection.statics.merge", Vec::new());
    for class in candidates {
        let selected = ctx.builder.create_named_block("reflection.statics.selected", Vec::new());
        let next = ctx.builder.create_named_block("reflection.statics.next", Vec::new());
        let literal = Expr::new(ExprKind::StringLiteral(class.clone()), expr.span);
        let candidate = lower_string_literal(ctx, &class, &literal);
        let equal = ctx.emit_value(Op::StrictEq, vec![name.value, candidate.value], None,
            PhpType::Bool, Op::StrictEq.default_effects(), Some(expr.span));
        ctx.builder.terminate(Terminator::CondBr { cond: equal.value,
            then_target: selected, then_args: Vec::new(), else_target: next, else_args: Vec::new() });
        ctx.builder.position_at_end(selected);
        let values = lower_reflection_class_get_static_properties(ctx, &class, &[], expr)
            .expect("a selected reflection class has a static-property map");
        store_value_into_temp(ctx, &result, result_type.clone(), values, expr.span);
        branch_to(ctx, merge);
        ctx.builder.position_at_end(next);
    }
    let empty = ctx.emit_value(Op::HashNew, Vec::new(), Some(Immediate::Capacity(0)),
        result_type.clone(), Op::HashNew.default_effects(), Some(expr.span));
    store_value_into_temp(ctx, &result, result_type, empty, expr.span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, &result, expr.span))
}
