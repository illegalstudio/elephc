//! Purpose:
//! Lowers class-introspection calls whose results are assembled from resolved AOT metadata.
//!
//! Called from:
//! - `super::function_calls::lower_function_call()` before ordinary registry lowering.
//!
//! Key details:
//! - `get_class_vars()` requires a literal class name, as enforced by its checker home.
//! - Property defaults are lowered as ordinary EIR expressions and boxed into fresh Mixed cells.

use super::*;

/// Lowers a direct `get_class_vars()` call into a fresh associative array of visible defaults.
pub(super) fn lower_static_get_class_vars(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "get_class_vars" {
        return None;
    }
    let class_name = literal_class_argument(args)?;
    let class_name = resolved_class_name(ctx, &class_name)?;
    let entries = visible_class_default_entries(ctx, &class_name);
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(entries.len() as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    for (property, default) in entries {
        let key = lower_string_literal(ctx, &property, expr);
        let value = match default {
            Some(default) => lower_expr(ctx, &default),
            None => lower_null(ctx, expr),
        };
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

/// Extracts the positional or named literal class argument from a checked call.
fn literal_class_argument(args: &[Expr]) -> Option<String> {
    let [argument] = args else {
        return None;
    };
    let argument = match &argument.kind {
        ExprKind::NamedArg { name, value } if php_symbol_key(name) == "class" => value.as_ref(),
        _ => argument,
    };
    match &argument.kind {
        ExprKind::StringLiteral(class_name) => {
            Some(class_name.trim_start_matches('\\').to_string())
        }
        ExprKind::ClassConstant { receiver } => match receiver {
            StaticReceiver::Named(name) => {
                Some(name.as_str().trim_start_matches('\\').to_string())
            }
            StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => None,
        },
        _ => None,
    }
}

/// Resolves a case-insensitive class-like name to its canonical declaration spelling.
fn resolved_class_name(ctx: &LoweringContext<'_, '_>, requested: &str) -> Option<String> {
    let key = php_symbol_key(requested);
    ctx.classes
        .keys()
        .chain(ctx.interfaces.keys())
        .chain(ctx.enums.keys())
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == key)
        .cloned()
}

/// Collects visible instance and static property defaults in physical declaration order.
fn visible_class_default_entries(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> Vec<(String, Option<Expr>)> {
    if ctx.interfaces.contains_key(class_name) {
        return Vec::new();
    }
    if ctx.enums.contains_key(class_name) && !ctx.classes.contains_key(class_name) {
        let mut entries = vec![("name".to_string(), None)];
        if ctx
            .enums
            .get(class_name)
            .is_some_and(|info| info.backing_type.is_some())
        {
            entries.push(("value".to_string(), None));
        }
        return entries;
    }
    let Some(info) = ctx.classes.get(class_name) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for (index, (property, _)) in info.properties.iter().enumerate() {
        if !seen.insert(property.clone()) || !instance_property_visible(ctx, class_name, info, property)
        {
            continue;
        }
        entries.push((property.clone(), info.defaults.get(index).cloned().flatten()));
    }
    for (index, (property, _)) in info.static_properties.iter().enumerate() {
        if !seen.insert(property.clone()) || !static_property_visible(ctx, class_name, info, property)
        {
            continue;
        }
        entries.push((
            property.clone(),
            info.static_defaults.get(index).cloned().flatten(),
        ));
    }
    entries
}

/// Returns whether an instance property is visible from the current lexical class.
fn instance_property_visible(
    ctx: &LoweringContext<'_, '_>,
    lookup_class: &str,
    info: &crate::types::ClassInfo,
    property: &str,
) -> bool {
    let declaring = info
        .property_declaring_classes
        .get(property)
        .map(String::as_str)
        .unwrap_or(lookup_class);
    let visibility = info
        .property_visibilities
        .get(property)
        .unwrap_or(&Visibility::Public);
    property_visible(ctx, declaring, visibility)
}

/// Returns whether a static property is visible from the current lexical class.
fn static_property_visible(
    ctx: &LoweringContext<'_, '_>,
    lookup_class: &str,
    info: &crate::types::ClassInfo,
    property: &str,
) -> bool {
    let declaring = info
        .static_property_declaring_classes
        .get(property)
        .map(String::as_str)
        .unwrap_or(lookup_class);
    let visibility = info
        .static_property_visibilities
        .get(property)
        .unwrap_or(&Visibility::Public);
    property_visible(ctx, declaring, visibility)
}

/// Applies PHP property visibility to one reflected default entry.
fn property_visible(
    ctx: &LoweringContext<'_, '_>,
    declaring_class: &str,
    visibility: &Visibility,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => ctx.current_class.as_deref() == Some(declaring_class),
        Visibility::Protected => ctx.current_class.as_deref().is_some_and(|current| {
            current == declaring_class || class_is_descendant(ctx, current, declaring_class)
        }),
    }
}

/// Returns whether one resolved class descends from another.
fn class_is_descendant(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    ancestor: &str,
) -> bool {
    let mut current = ctx.classes.get(class_name).and_then(|info| info.parent.as_deref());
    while let Some(parent) = current {
        if parent == ancestor {
            return true;
        }
        current = ctx.classes.get(parent).and_then(|info| info.parent.as_deref());
    }
    false
}
