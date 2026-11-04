//! Purpose:
//! Exposes implicit integer conversions of checked arithmetic as scalar EIR sinks.
//!
//! Called from:
//! - Declared `int` instance-property stores in `stmt::instance_property_writes`.
//! - Typed destination and declared-local assignment lowering.
//!
//! Key details:
//! - Only closed add/sub/mul trees with concrete integer leaves are accepted.
//! - Conversion happens after the whole tree, preserving intermediate float promotion.
//! - Unknown Mixed values and reference places retain their existing boundary handling.
//! - A declared `int` property receives a PHP-checked narrowing, never a silent cast.

use super::*;

/// Lowers parameter storage, preserving implicit callable identity and integer destinations.
pub(super) fn lower_expr_for_parameter(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    destination: &PhpType,
) -> LoweredValue {
    if destination.codegen_repr() == PhpType::Callable {
        let source = match &expr.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => expr,
        };
        if let ExprKind::StringLiteral(name) = &source.kind {
            if let Some(target) = crate::types::param_binding::callable_target_from_name(name) {
                return lower_callable_value(ctx, &target, source);
            }
        }
    }
    let value = lower_expr_for_destination(ctx, expr, destination);
    coerce_integer_arithmetic(ctx, value, destination, expr.span)
}

/// Makes a checked arithmetic tree's existing integer destination conversion explicit.
pub(crate) fn coerce_integer_arithmetic(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    destination: &PhpType,
    span: Span,
) -> LoweredValue {
    if destination != &PhpType::Int || !checked_integer_tree(ctx, value.value) {
        return value;
    }
    coerce_to_int_at_span(ctx, value, Some(span))
}

/// Requires every noninteger node to be checked arithmetic rather than a dynamic PHP value.
fn checked_integer_tree(ctx: &LoweringContext<'_, '_>, root: ValueId) -> bool {
    if ctx.builder.value_type(root) == IrType::I64 {
        return false;
    }
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    while let Some(value) = pending.pop() {
        if !seen.insert(value) { continue; }
        if ctx.builder.value_type(value) == IrType::I64
            && ctx.builder.value_php_type(value) == PhpType::Int
        {
            continue;
        }
        let Some(inst) = ctx.builder.value_defining_instruction(value) else { return false; };
        let arithmetic = matches!(inst.op, Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul)
            || (inst.op == Op::MixedNumericBinop && matches!(inst.immediate,
                Some(Immediate::MixedNumericOp(MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul))));
        if !arithmetic || inst.operands.len() != 2 { return false; }
        pending.extend(inst.operands.iter().copied());
    }
    true
}

/// Narrows checked integer arithmetic stored into a declared `int` property before the store.
///
/// The boxed result otherwise reaches `PropSet` as `Mixed`, where the backend's weak
/// typed-property guard unboxes it after the producer has already allocated its heap cell.
/// `Op::PropertyBoundaryCheckedToInt` performs the same verdict (an int stores; an overflowed
/// float throws `Cannot assign float to property C::$p of type int`) while handing the store a
/// raw word, so the frame-cell planner can keep the producer off the heap.
///
/// Only a concrete declared, non-readonly, non-reference `int` slot qualifies. Callers invoke
/// this after the magic `__set`, property-hook and inherited-setter routes have been selected,
/// so those paths keep the original PHP tag. Unions, nullable types and dynamic receivers keep
/// the guard on the boxed value. The message names the declaring class, as php-src does.
pub(crate) fn narrow_checked_integer_for_int_property(
    ctx: &mut LoweringContext<'_, '_>,
    object: ValueId,
    property: &str,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if ctx.builder.value_php_type(value.value).codegen_repr() != PhpType::Mixed
        || !checked_integer_tree(ctx, value.value)
    {
        return value;
    }
    let PhpType::Object(class_name) = ctx.builder.value_php_type(object).codegen_repr() else {
        return value;
    };
    let normalized = class_name.trim_start_matches('\\');
    let Some(info) = ctx.classes.get(normalized) else { return value; };
    let Some((index, (name, php_type))) = info.visible_property(property) else { return value; };
    if *php_type != PhpType::Int
        || !info.property_slot_is_declared(index, name)
        || info.property_slot_is_reference(index, name)
        || info.is_readonly_class
        || info.readonly_properties.contains(name)
    {
        return value;
    }
    let declaring = info
        .property_declaring_classes
        .get(name)
        .map(String::as_str)
        .unwrap_or(normalized)
        .trim_start_matches('\\');
    let message = format!("Cannot assign float to property {declaring}::${name} of type int");
    let data = ctx.intern_string(&message);
    let narrowed = ctx.emit_value(
        Op::PropertyBoundaryCheckedToInt,
        vec![value.value],
        Some(Immediate::Data(data)),
        PhpType::Int,
        Op::PropertyBoundaryCheckedToInt.default_effects(),
        Some(span),
    );
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    narrowed
}
