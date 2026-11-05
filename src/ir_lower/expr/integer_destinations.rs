//! Purpose:
//! Exposes implicit integer conversions of checked arithmetic as scalar EIR sinks.
//!
//! Called from:
//! - Declared `int` instance-property stores in `stmt::instance_property_writes`.
//! - Typed destination and declared-local assignment lowering.
//! - Call argument lowering, which also exposes conversions at source-typed `float` parameters.
//!
//! Key details:
//! - Only closed add/sub/mul trees with concrete integer leaves are accepted.
//! - Conversion happens after the whole tree, preserving intermediate float promotion.
//! - Unknown Mixed values and reference places retain their existing boundary handling.
//! - A declared `int` property receives a PHP-checked narrowing, never a silent cast.

use super::*;

/// Returns whether parameter `index` carries a source-written type declaration.
///
/// Builtin registry signatures never do, so their arguments keep the operand shapes that
/// builtin-specific passes match (for example the boxed `sqrt($i + c)` range reducer).
pub(super) fn parameter_is_source_typed(sig: &crate::types::FunctionSig, index: usize) -> bool {
    sig.param_type_exprs.get(index).is_some_and(Option::is_some)
}

/// Lowers parameter storage, preserving implicit callable identity and integer destinations.
///
/// `source_typed` admits the float conversion only for user-declared `float` parameters.
pub(super) fn lower_expr_for_parameter(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    destination: &PhpType,
    source_typed: bool,
) -> LoweredValue {
    if let Some(lowered) = lower_mixed_union_parameter(ctx, expr, destination) {
        return lowered;
    }
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
    if source_typed && destination == &PhpType::Float {
        return coerce_float_arithmetic(ctx, value, expr.span);
    }
    coerce_integer_arithmetic(ctx, value, destination, expr.span)
}

/// Exposes a float parameter's conversion of a checked arithmetic tree as one scalar cast.
///
/// PHP passes an int|float argument to a `float` parameter as `(float)` of the whole result,
/// so an overflowed tree keeps its promoted double and an in-range integer converts exactly
/// once, matching the boxed call-boundary conversion.
fn coerce_float_arithmetic(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if !checked_integer_tree(ctx, value.value) {
        return value;
    }
    coerce_to_float_at_span(ctx, value, Some(span))
}

/// Applies PHP's runtime union-member selection to a boxed `Mixed` place.
///
/// A local or declared property whose storage widened to `Mixed` (an untyped parameter,
/// or a union-typed local) still has to honor the callee's declared union. The checker accepts those
/// arguments because `Mixed` is compatible with every member, but the declared type is
/// flattened to `Mixed` in the parameter slot, so the concrete EIR cast arm cannot see
/// which member PHP would choose. Build the same weak-mode selection expression the enum
/// argument path uses, guard it with the matching runtime acceptance predicate, and raise
/// a catchable `TypeError` for a value that fits no member.
///
/// Only a side-effect-free place (a local, declared property, or element of those) is
/// eligible: the generated predicate and selection expression read it more than once,
/// exactly like the existing enum argument binding. `static_place_type` is what proves the
/// read cannot invoke a magic accessor.
fn lower_mixed_union_parameter(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    destination: &PhpType,
) -> Option<LoweredValue> {
    let PhpType::Union(members) = destination else {
        return None;
    };
    if members.contains(&PhpType::Mixed) {
        return None;
    }
    let source_ty = super::ref_place_args::static_place_type(ctx, expr)?;
    if source_ty.codegen_repr() != PhpType::Mixed {
        return None;
    }
    let bound = crate::types::param_binding::runtime_scalar_param_binding(destination, expr)?;
    let accepted = super::enum_argument_guards::accepts_type(destination, expr, expr.span)?;
    let accepted = lower_expr(ctx, &accepted);
    let ok = ctx.builder.create_named_block("param.union.ok", Vec::new());
    let bad = ctx.builder.create_named_block("param.union.bad", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: accepted.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: bad,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(bad);
    let expected = crate::types::diagnostic_names::php_type_name(destination);
    let actual = super::enum_argument_guards::runtime_debug_type(expr, expr.span);
    let message = super::enum_argument_guards::concat(
        Expr::new(
            ExprKind::StringLiteral(format!("Argument must be of type {expected}, ")),
            expr.span,
        ),
        super::enum_argument_guards::concat(
            actual,
            Expr::new(ExprKind::StringLiteral(" given".to_string()), expr.span),
            expr.span,
        ),
        expr.span,
    );
    super::enum_argument_guards::throw_message(ctx, "TypeError", message, expr.span);
    ctx.builder.position_at_end(ok);
    Some(lower_expr_for_destination(ctx, &bound, destination))

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
