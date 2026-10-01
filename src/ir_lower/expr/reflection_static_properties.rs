//! Purpose:
//! ReflectionClass static-property reads, writes, and metadata.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;
use std::collections::HashMap;

/// Lowers `ReflectionClass::getStaticProperties()` to a live static-property map.
pub(super) fn lower_reflection_class_get_static_properties(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !args.is_empty() {
        return None;
    }
    let properties = reflection_class_static_property_map_entries(ctx, class_name)?;
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(properties.len() as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    for (property, declaring_class, property_ty) in properties {
        let key_expr = Expr::new(ExprKind::StringLiteral(property.clone()), expr.span);
        let key = lower_string_literal(ctx, &property, &key_expr);
        let value = lower_reflection_static_property_get_by_class_name(
            ctx,
            &declaring_class,
            &property,
            property_ty,
            expr,
        );
        let value = box_value_as_mixed(ctx, value, expr.span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(expr.span),
        );
    }
    Some(hash)
}

/// Lowers `ReflectionClass::getStaticPropertyValue()` to a live static-property read.
pub(super) fn lower_reflection_class_get_static_property_value(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let (property, default) = reflection_class_get_static_property_value_args(args)?;
    if let Some((declaring_class, property_ty)) =
        reflection_class_static_property_target(ctx, class_name, &property)
    {
        if default.is_none() {
            return Some(lower_reflection_static_property_get_by_class_name(
                ctx,
                &declaring_class,
                &property,
                property_ty,
                expr,
            ));
        }
        return None;
    }
    Some(match default {
        Some(default) => lower_expr(ctx, &default),
        None => lower_reflection_class_missing_static_property(ctx, class_name, &property, expr),
    })
}

/// Lowers `ReflectionClass::setStaticPropertyValue()` to a live static-property write.
pub(super) fn lower_reflection_class_set_static_property_value(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let (property, value) = reflection_class_set_static_property_value_args(args)?;
    let (declaring_class, _) = reflection_class_static_property_target(ctx, class_name, &property)?;
    let value = lower_expr(ctx, &value);
    store_reflection_static_property_by_class_name(
        ctx,
        &declaring_class,
        &property,
        value.value,
        expr.span,
    );
    Some(lower_null(ctx, expr))
}

/// Lowers a missing static-property lookup to PHP's catchable ReflectionException.
pub(super) fn lower_reflection_class_missing_static_property(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    let message = format!(
        "Property {}::${} does not exist",
        class_name.trim_start_matches('\\'),
        property
    );
    let exception = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("ReflectionException"),
            args: vec![Expr::new(ExprKind::StringLiteral(message), expr.span)],
        },
        expr.span,
    );
    let placeholder = lower_null(ctx, expr);
    let exception = lower_expr(ctx, &exception);
    ctx.builder.terminate(Terminator::Throw {
        value: exception.value,
    });
    placeholder
}

/// Returns synthetic array entries for current static-property values on a reflected class.
pub(super) fn reflection_class_static_property_map_entries(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> Option<Vec<(String, String, PhpType)>> {
    let class_name = class_name.trim_start_matches('\\');
    let class_info = ctx.classes.get(class_name)?;
    let ranks = reflection_class_property_ranks(ctx, class_name);
    let mut entries: Vec<((usize, usize), usize, (String, String, PhpType))> = class_info
        .static_properties
        .iter()
        .enumerate()
        .filter_map(|(position, (property, property_ty))| {
            let declaring_class = class_info
                .static_property_declaring_classes
                .get(property)
                .cloned()
                .unwrap_or_else(|| class_name.to_string());
            // An ancestor's private static is not a property of this class in PHP.
            let private = class_info.static_property_visibilities.get(property)
                == Some(&crate::parser::ast::Visibility::Private);
            if private && php_symbol_key(&declaring_class) != php_symbol_key(class_name) {
                return None;
            }
            let rank = ranks
                .get(&php_symbol_key(&declaring_class))
                .map(|(depth, order)| (*depth, order.get(property).copied().unwrap_or(usize::MAX)))
                .unwrap_or((0, usize::MAX));
            let property_ty = normalize_value_php_type(property_ty.codegen_repr());
            Some((rank, position, (property.clone(), declaring_class, property_ty)))
        })
        .collect();
    // The storage layout puts an ancestor's statics first and keeps a redeclared static in its
    // ancestor's slot; PHP lists the class's own statics first, in the order it declares them.
    entries.sort_by_key(|(rank, position, _)| (*rank, *position));
    Some(entries.into_iter().map(|(_, _, entry)| entry).collect())
}

/// Returns, for `class_name` and each ancestor keyed by `php_symbol_key`, its depth (0 for the
/// class, 1 for its parent) and its own property declaration order, walking the chain once.
fn reflection_class_property_ranks(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> HashMap<String, (usize, HashMap<String, usize>)> {
    let mut ranks = HashMap::new();
    let mut current = Some(class_name.trim_start_matches('\\').to_string());
    let mut depth = 0;
    while let Some(name) = current {
        let info = ctx.classes.get(name.as_str());
        let order = info
            .map(|info| {
                info.property_order
                    .iter()
                    .enumerate()
                    .map(|(index, property)| (property.clone(), index))
                    .collect()
            })
            .unwrap_or_default();
        ranks.entry(php_symbol_key(&name)).or_insert((depth, order));
        current = info.and_then(|info| info.parent.clone());
        depth += 1;
    }
    ranks
}

/// Boxes a concrete PHP value into the runtime `Mixed` cell representation.
pub(super) fn box_value_as_mixed(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Mixed {
        return value;
    }
    ctx.emit_value(
        Op::MixedBox,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
}

/// Returns the literal property name and optional explicit default argument for a get call.
pub(super) fn reflection_class_get_static_property_value_args(
    args: &[Expr],
) -> Option<(String, Option<Expr>)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (name, default) =
        reflection_class_static_property_regular_args(&args, "name", Some("default"))?;
    let property = reflection_class_static_property_name_arg(name.as_ref()?)?;
    Some((property, default))
}

/// Returns the literal property name and value expression for a set call.
pub(super) fn reflection_class_set_static_property_value_args(args: &[Expr]) -> Option<(String, Expr)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (name, value) =
        reflection_class_static_property_regular_args(&args, "name", Some("value"))?;
    let property = reflection_class_static_property_name_arg(name.as_ref()?)?;
    let value = value?;
    Some((property, value))
}

/// Normalizes supported static-property method arguments into parameter order.
pub(super) fn reflection_class_static_property_regular_args(
    args: &[Expr],
    first_name: &str,
    second_name: Option<&str>,
) -> Option<(Option<Expr>, Option<Expr>)> {
    if !crate::types::call_args::has_named_args(args) {
        return match args {
            [first] => Some((Some(first.clone()), None)),
            [first, second] => Some((Some(first.clone()), Some(second.clone()))),
            _ => None,
        };
    }

    let mut first = None;
    let mut second = None;
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value } if php_symbol_key(name) == first_name => {
                first = Some((**value).clone());
            }
            ExprKind::NamedArg { name, value }
                if second_name.is_some_and(|expected| php_symbol_key(name) == expected) =>
            {
                second = Some((**value).clone());
            }
            _ => return None,
        }
    }
    Some((first, second))
}

/// Extracts a literal property name from a ReflectionClass static-property call argument.
pub(super) fn reflection_class_static_property_name_arg(arg: &Expr) -> Option<String> {
    match &arg.kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        _ => None,
    }
}

/// Returns the declaring class and retained PHP type for one reflected static property.
pub(super) fn reflection_class_static_property_target(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> Option<(String, PhpType)> {
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    let property_ty = class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, property_ty)| normalize_value_php_type(property_ty.codegen_repr()))?;
    let declaring_class = class_info
        .static_property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    Some((declaring_class, property_ty))
}

/// Emits a visibility-bypassing reflection static-property read.
pub(super) fn lower_reflection_static_property_get_by_class_name(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    result_type: PhpType,
    expr: &Expr,
) -> LoweredValue {
    lower_static_property_get_by_class_name_with_op(
        ctx,
        class_name,
        property,
        result_type,
        expr,
        Op::LoadReflectionStaticProperty,
    )
}

/// Emits a visibility-bypassing static-property initialization probe.
pub(super) fn lower_reflection_static_property_initialized_by_class_name(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    lower_static_property_get_by_class_name_with_op(
        ctx,
        class_name,
        property,
        PhpType::Bool,
        expr,
        Op::ReflectionStaticPropertyInitialized,
    )
}

/// Emits a static-property read using the requested static-property opcode.
pub(super) fn lower_static_property_get_by_class_name_with_op(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    result_type: PhpType,
    expr: &Expr,
    op: Op,
) -> LoweredValue {
    let data = ctx.intern_string(&format!("{}::{}", class_name, property));
    ctx.emit_value(
        op,
        Vec::new(),
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    )
}

/// Emits a visibility-bypassing reflection static-property write.
pub(super) fn store_reflection_static_property_by_class_name(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    value: ValueId,
    span: Span,
) {
    store_static_property_by_class_name_with_op(
        ctx,
        class_name,
        property,
        value,
        span,
        Op::StoreReflectionStaticProperty,
    );
}

/// Emits a static-property write using the requested static-property opcode.
pub(super) fn store_static_property_by_class_name_with_op(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    value: ValueId,
    span: Span,
    op: Op,
) {
    let data = ctx.intern_string(&format!("{}::{}", class_name, property));
    ctx.emit_void(
        op,
        vec![value],
        Some(Immediate::Data(data)),
        op.default_effects(),
        Some(span),
    );
}

