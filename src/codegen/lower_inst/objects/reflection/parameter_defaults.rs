//! Purpose:
//! Reflection parameter defaults and ReflectionType metadata conversion.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Applies declaration-time int-to-float binding only to defaults PHP can fold without constants.
pub(super) fn reflection_folded_integer_default(
    source: &Expr,
    ty: &PhpType,
    ctx: &FunctionContext<'_>,
    current_class: &str,
) -> Option<ReflectionParameterDefaultValue> {
    if !ty.coerces_integer_to_float() { return None; }
    crate::optimize::with_expression_semantics(ctx.module.semantic_profile, source.source_profile, || {
        let mut source = source.clone();
        source.producer_binding = None;
        let mut folded = crate::optimize::fold_constant_expression(source);
        crate::types::deferred_defaults::prune_literal_branches(&mut folded);
        let folded = crate::optimize::fold_constant_expression(folded);
        let value = crate::codegen::const_default_values::resolve_literal_expression(
            &folded, ctx.module, (!current_class.is_empty()).then_some(current_class),
        )?;
        let ExprKind::IntLiteral(value) = value else { return None; };
        Some(ReflectionParameterDefaultValue::Float(value as f64))
    })
}

/// Converts a supported parameter default expression into Reflection metadata.
pub(super) fn reflection_parameter_default_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    default: &Expr,
) -> Result<Option<ReflectionParameterDefaultValue>> {
    if let Some(value) = reflection_deferred_default_value(default) {
        return Ok(Some(value));
    }
    let helper_for_default = |function: &&crate::ir::Function| (function.lexical_class.is_none() && current_class.is_empty()
            || function.lexical_class.as_deref().is_some_and(|scope|
            scope == current_class || current_info.is_some_and(|info|
                info.method_impl_classes.values().chain(info.static_method_impl_classes.values())
                    .any(|owner| owner == scope))))
            && function.callable_default.as_ref()
                .is_some_and(|value| value.same_value_producer(default));
    // A default that reads a deprecated constant has a reflection twin, which reports on every
    // call at the caller's line instead of consuming the parameter's own once flags.
    let helpers = || ctx.module.functions.iter().chain(ctx.module.closures.iter()).filter(helper_for_default);
    if let Some(helper) = helpers()
        .find(|function| crate::ir_lower::is_reflection_default_helper(&function.name))
        .or_else(|| helpers().next())
    {
        return Ok(Some(ReflectionParameterDefaultValue::EirHelper(helper.name.clone())));
    }
    if let Some(value) =
        reflection_object_parameter_default_value(ctx, current_class, current_info, default)?
    {
        return Ok(Some(value));
    }
    if let Some(value) = reflection_literal_parameter_default_value(default, ctx.module.semantic_profile) {
        return Ok(Some(value));
    }
    let context = crate::codegen::const_default_values::ConstDefaultContext::new(ctx.module, (!current_class.is_empty()).then_some(current_class));
    if let Some(kind) = crate::codegen::const_default_values::resolve_const_default(default, &context)
        .and_then(crate::codegen::const_default_values::resolved_literal_default)
    {
        return Ok(reflection_literal_parameter_default_value(&Expr::new(kind, default.span), ctx.module.semantic_profile));
    }
    match &default.kind {
        // Unsupported constant forms retain their source export without failing compilation.
        // Representable scalar and array values share the recursive constant evaluator.
        ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. } => Ok(
            reflection_constant_value(ctx, current_class, current_info, default, 0)
                .ok()
                .and_then(reflection_parameter_default_from_constant_value),
        ),
        _ => Ok(None),
    }
}

/// Retains the message from a compiler-generated lazy default Error without evaluating it.
pub(super) fn reflection_deferred_default_value(default: &Expr) -> Option<ReflectionParameterDefaultValue> {
    let ExprKind::Throw(exception) = &default.kind else { return None; };
    let ExprKind::NewObject { class_name, args } = &exception.kind else { return None; };
    let [Expr { kind: ExprKind::StringLiteral(message), .. }] = args.as_slice() else {
        return None;
    };
    (class_name.as_str() == "Error")
        .then(|| ReflectionParameterDefaultValue::DeferredError(message.clone()))
}

/// Converts a supported non-object parameter default expression into metadata.
pub(super) fn reflection_parameter_default_non_object_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    default: &Expr,
) -> Result<Option<ReflectionParameterDefaultValue>> {
    if let Some(value) = reflection_literal_parameter_default_non_object_value(default, ctx.module.semantic_profile) {
        return Ok(Some(value));
    }
    let context = crate::codegen::const_default_values::ConstDefaultContext::new(ctx.module, (!current_class.is_empty()).then_some(current_class));
    if let Some(kind) = crate::codegen::const_default_values::resolve_const_default(default, &context)
        .and_then(crate::codegen::const_default_values::resolved_literal_default)
    {
        return Ok(reflection_literal_parameter_default_non_object_value(&Expr::new(kind, default.span), ctx.module.semantic_profile));
    }
    match &default.kind {
        // Object arguments share the supported constant-value fallback.
        ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. } => Ok(
            reflection_constant_value(ctx, current_class, current_info, default, 0)
                .ok()
                .and_then(reflection_parameter_default_from_constant_value),
        ),
        _ => Ok(None),
    }
}

/// Converts constructor arguments for object defaults, rejecting nested objects.
pub(super) fn reflection_literal_parameter_default_non_object_value(
    default: &Expr,
    profile: elephc_semantics::SemanticProfile,
) -> Option<ReflectionParameterDefaultValue> {
    let value = reflection_literal_parameter_default_value(default, profile)?;
    if reflection_default_value_contains_object(&value) {
        return None;
    }
    Some(value)
}

/// Returns whether a retained default contains an object value.
pub(super) fn reflection_default_value_contains_object(value: &ReflectionParameterDefaultValue) -> bool {
    match value {
        ReflectionParameterDefaultValue::Object { .. }
        | ReflectionParameterDefaultValue::EnumCase(_)
        // Its singleton reads are object materialization, even when the result is scalar.
        | ReflectionParameterDefaultValue::Evaluated { .. } => true,
        ReflectionParameterDefaultValue::Array(elements) => elements
            .iter()
            .any(reflection_default_value_contains_object),
        ReflectionParameterDefaultValue::AssocArray(entries) => entries
            .iter()
            .any(|entry| reflection_default_value_contains_object(&entry.value)),
        ReflectionParameterDefaultValue::Int(_)
        | ReflectionParameterDefaultValue::Bool(_)
        | ReflectionParameterDefaultValue::Float(_)
        | ReflectionParameterDefaultValue::Str(_)
        | ReflectionParameterDefaultValue::DeferredError(_)
        | ReflectionParameterDefaultValue::EirHelper(_)
        | ReflectionParameterDefaultValue::Null => false,
    }
}

/// Converts a literal parameter/property default expression into Reflection metadata.
pub(super) fn reflection_literal_parameter_default_value(
    default: &Expr,
    profile: elephc_semantics::SemanticProfile,
) -> Option<ReflectionParameterDefaultValue> {
    if let Some(ExprKind::StringLiteral(value)) =
        crate::codegen::literal_defaults::fold_named_class_constant(&default.kind)
    {
        return Some(ReflectionParameterDefaultValue::Str(value));
    }
    if let Some(folded) = crate::types::fold_spreadless_mixed_array_literal(&default.kind, crate::codegen_support::compile_php_version(), profile) {
        return reflection_literal_parameter_default_value(&Expr::new(folded, default.span), profile);
    }
    match &default.kind {
        ExprKind::IntLiteral(value) => Some(ReflectionParameterDefaultValue::Int(*value)),
        ExprKind::BoolLiteral(value) => Some(ReflectionParameterDefaultValue::Bool(*value)),
        ExprKind::FloatLiteral(value) => Some(ReflectionParameterDefaultValue::Float(*value)),
        ExprKind::StringLiteral(value) => Some(ReflectionParameterDefaultValue::Str(value.clone())),
        ExprKind::Null => Some(ReflectionParameterDefaultValue::Null),
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .map(|value| reflection_literal_parameter_default_value(value, profile))
            .collect::<Option<Vec<_>>>()
            .map(ReflectionParameterDefaultValue::Array),
        ExprKind::ArrayLiteralAssoc(entries) => reflection_assoc_array_default_value(entries, profile),
        ExprKind::UnaryPlus(inner) => crate::semantics::numeric::literal_unary_plus(inner)
            .and_then(|number| reflection_literal_parameter_default_value(&number, profile)),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value
                .checked_neg()
                .map(ReflectionParameterDefaultValue::Int),
            ExprKind::FloatLiteral(value) => Some(ReflectionParameterDefaultValue::Float(-value)),
            _ => None,
        },
        _ => None,
    }
}

/// Converts an associative array literal into normalized Reflection default metadata.
pub(super) fn reflection_assoc_array_default_value(
    entries: &[(Expr, Expr)],
    profile: elephc_semantics::SemanticProfile,
) -> Option<ReflectionParameterDefaultValue> {
    entries
        .iter()
        .map(|(key, value)| {
            let key = reflection_default_array_key(key, profile)?;
            let value = reflection_literal_parameter_default_value(value, profile)?;
            Some(ReflectionDefaultAssocEntry { key, value })
        })
        .collect::<Option<Vec<_>>>()
        .map(ReflectionParameterDefaultValue::AssocArray)
}

/// Converts one supported associative-array key expression into PHP-normalized metadata.
pub(super) fn reflection_default_array_key(key: &Expr, profile: elephc_semantics::SemanticProfile) -> Option<ReflectionDefaultArrayKey> {
    if let Some(ExprKind::StringLiteral(value)) =
        crate::codegen::literal_defaults::fold_named_class_constant(&key.kind)
    {
        return reflection_default_string_array_key(&value, profile);
    }
    let key = crate::codegen::const_default_values::resolve_literal_array_const_default_key(key, profile)?;
    Some(reflection_default_key_from_shared(key))
}

/// Shares byte-based identity with callable and eval default metadata.
pub(super) fn reflection_default_string_array_key(value: &str, profile: elephc_semantics::SemanticProfile) -> Option<ReflectionDefaultArrayKey> {
    crate::codegen::const_default_values::const_default_string_array_key(value, profile)
        .map(reflection_default_key_from_shared)
}

/// Adapts the shared key representation without reclassifying source strings.
fn reflection_default_key_from_shared(key: crate::codegen::const_default_values::ConstDefaultArrayKey) -> ReflectionDefaultArrayKey {
    match key {
        crate::codegen::const_default_values::ConstDefaultArrayKey::Int(value) => ReflectionDefaultArrayKey::Int(value),
        crate::codegen::const_default_values::ConstDefaultArrayKey::String(value) => ReflectionDefaultArrayKey::Str(value),
    }
}

/// Converts scalar/null constant metadata into a parameter default value.
pub(super) fn reflection_parameter_default_from_constant_value(
    value: ReflectionConstantValue,
) -> Option<ReflectionParameterDefaultValue> {
    match value {
        ReflectionConstantValue::Int(value) => Some(ReflectionParameterDefaultValue::Int(value)),
        ReflectionConstantValue::Bool(value) => Some(ReflectionParameterDefaultValue::Bool(value)),
        ReflectionConstantValue::Float(value) => {
            Some(ReflectionParameterDefaultValue::Float(value))
        }
        ReflectionConstantValue::Str(value) => Some(ReflectionParameterDefaultValue::Str(value)),
        ReflectionConstantValue::Null => Some(ReflectionParameterDefaultValue::Null),
        ReflectionConstantValue::EnumCase { .. } => None,
        ReflectionConstantValue::Evaluated { value, enum_cases } => {
            Some(ReflectionParameterDefaultValue::Evaluated {
                value: Box::new(reflection_parameter_default_from_constant_value(*value)?),
                enum_cases,
            })
        }
        ReflectionConstantValue::Array(elements) => elements
            .into_iter()
            .map(reflection_parameter_default_from_constant_value)
            .collect::<Option<Vec<_>>>()
            .map(ReflectionParameterDefaultValue::Array),
        ReflectionConstantValue::AssocArray(entries) => entries
            .into_iter()
            .map(|entry| {
                Some(ReflectionDefaultAssocEntry {
                    key: entry.key,
                    value: reflection_parameter_default_from_constant_value(entry.value)?,
                })
            })
            .collect::<Option<Vec<_>>>()
            .map(ReflectionParameterDefaultValue::AssocArray),
    }
}

/// Returns PHP's constant-name metadata for parameter defaults that name a constant.
pub(super) fn reflection_parameter_default_constant_name(default: &Expr) -> Option<String> {
    match &default.kind {
        ExprKind::ConstRef(name) => Some(name.as_str().trim_start_matches('\\').to_string()),
        _ => crate::types::deferred_defaults::source_constant_name(default),
    }
}

/// Converts a normalized parameter type into a supported `ReflectionType` subset.
pub(super) fn reflection_parameter_type_metadata(
    type_expr: Option<&TypeExpr>,
    ty: &PhpType,
) -> Option<ReflectionParameterTypeMetadata> {
    if *ty == PhpType::Mixed {
        if let Some(type_expr) = type_expr {
            // A widened `mixed` ABI slot can still carry the source declaration in
            // `param_type_exprs`; reflection must report the declared syntax, not the
            // storage representation. A genuine `mixed` annotation falls through.
            let declared_mixed = matches!(
                type_expr,
                TypeExpr::Named(name)
                    if name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("mixed")
            );
            if !declared_mixed {
                if let Some(metadata) = reflection_declared_type_metadata(type_expr) {
                    return Some(metadata);
                }
            }
        }
    }
    if let Some(TypeExpr::Intersection(members)) = type_expr {
        return reflection_intersection_type_metadata(members);
    }
    match ty {
        PhpType::Union(members) => reflection_union_or_nullable_type_metadata(members),
        _ => reflection_named_type_metadata(ty).map(ReflectionParameterTypeMetadata::Named),
    }
}

/// Converts a retained declared class-constant type into reflection metadata.
pub(super) fn reflection_declared_type_metadata(
    type_expr: &TypeExpr,
) -> Option<ReflectionParameterTypeMetadata> {
    match type_expr {
        TypeExpr::Nullable(inner) => {
            let mut metadata = reflection_named_type_metadata_from_type_expr(inner)?;
            metadata.allows_null = true;
            Some(ReflectionParameterTypeMetadata::Named(metadata))
        }
        TypeExpr::Union(members) => {
            let allows_null = members.iter().any(|member| matches!(member, TypeExpr::Void));
            let mut types = members
                .iter()
                .filter(|member| !matches!(member, TypeExpr::Void))
                .map(reflection_named_type_metadata_from_type_expr)
                .collect::<Option<Vec<_>>>()?;
            sort_union_members_like_php(&mut types);
            if types.len() == 1 {
                let mut metadata = types.into_iter().next()?;
                metadata.allows_null = allows_null;
                Some(ReflectionParameterTypeMetadata::Named(metadata))
            } else {
                (!types.is_empty()).then_some(ReflectionParameterTypeMetadata::Union(
                    ReflectionUnionTypeMetadata { types, allows_null },
                ))
            }
        }
        TypeExpr::Intersection(members) => reflection_intersection_type_metadata(members),
        _ => reflection_named_type_metadata_from_type_expr(type_expr)
            .map(ReflectionParameterTypeMetadata::Named),
    }
}

/// Converts a declared return type into the supported `ReflectionType` subset.
pub(super) fn reflection_return_type_metadata(sig: &FunctionSig) -> Option<ReflectionParameterTypeMetadata> {
    if !sig.declared_return {
        return None;
    }
    match &sig.return_type {
        PhpType::Void => Some(ReflectionParameterTypeMetadata::Named(
            reflection_builtin_named_type("void", false),
        )),
        PhpType::Never => Some(ReflectionParameterTypeMetadata::Named(
            reflection_builtin_named_type("never", false),
        )),
        PhpType::Union(members) => reflection_union_or_nullable_type_metadata(members),
        ty => reflection_named_type_metadata(ty).map(ReflectionParameterTypeMetadata::Named),
    }
}

/// Converts a method return contract while preserving reflection-visible late-bound `static`.
pub(super) fn reflection_method_return_type_metadata(
    sig: &FunctionSig,
    late_static_return: Option<&TypeExpr>,
) -> Option<ReflectionParameterTypeMetadata> {
    if sig.declared_return {
        if let Some(return_type) = late_static_return {
            let mut metadata = reflection_declared_type_metadata(return_type)?;
            if let ReflectionParameterTypeMetadata::Union(union) = &mut metadata {
                // PHP reports named class/interface members before `static`, then builtins.
                union.types.sort_by_key(|member| {
                    if member.name.eq_ignore_ascii_case("static") {
                        1
                    } else if member.is_builtin {
                        2
                    } else {
                        0
                    }
                });
            }
            return Some(metadata);
        }
    }
    reflection_return_type_metadata(sig)
}

/// Converts a normalized non-union parameter type into a simple `ReflectionNamedType`.
pub(super) fn reflection_named_type_metadata(ty: &PhpType) -> Option<ReflectionNamedTypeMetadata> {
    if ty.is_php_array() {
        return Some(reflection_builtin_named_type("array", false));
    }
    match ty {
        PhpType::Int => Some(reflection_builtin_named_type("int", false)),
        PhpType::Float => Some(reflection_builtin_named_type("float", false)),
        PhpType::Str => Some(reflection_builtin_named_type("string", false)),
        PhpType::Bool => Some(reflection_builtin_named_type("bool", false)),
        PhpType::False => Some(reflection_builtin_named_type("false", false)),
        PhpType::Iterable => Some(reflection_builtin_named_type("iterable", false)),
        PhpType::Mixed => Some(reflection_builtin_named_type("mixed", true)),
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            Some(reflection_builtin_named_type("array", false))
        }
        PhpType::Callable | PhpType::DeclaredCallable => {
            Some(reflection_builtin_named_type("callable", false))
        }
        PhpType::Object(name) if name.is_empty() => {
            Some(reflection_builtin_named_type("object", false))
        }
        PhpType::Object(name) => Some(ReflectionNamedTypeMetadata {
            name: name.clone(),
            allows_null: false,
            is_builtin: false,
        }),
        _ => None,
    }
}

/// Builds metadata for one builtin named type.
pub(super) fn reflection_builtin_named_type(name: &str, allows_null: bool) -> ReflectionNamedTypeMetadata {
    ReflectionNamedTypeMetadata {
        name: name.to_string(),
        allows_null,
        is_builtin: true,
    }
}

/// Handles `T|null` as a nullable named type and wider unions as `ReflectionUnionType`.
pub(super) fn reflection_union_or_nullable_type_metadata(
    members: &[PhpType],
) -> Option<ReflectionParameterTypeMetadata> {
    let allows_null = members.iter().any(|member| matches!(member, PhpType::Void));
    let mut types = Vec::<ReflectionNamedTypeMetadata>::new();
    for member in members
        .iter()
        .filter(|member| !matches!(member, PhpType::Void))
    {
        let metadata = reflection_named_type_metadata(member)?;
        // Packed and keyed storage are one PHP type, including inside nullable/wider unions.
        if !types.iter().any(|existing| {
            existing.name == metadata.name && existing.is_builtin == metadata.is_builtin
        }) {
            types.push(metadata);
        }
    }
    if types.len() == 1 {
        let mut metadata = types.pop()?;
        metadata.allows_null = allows_null;
        return Some(ReflectionParameterTypeMetadata::Named(metadata));
    }
    // PHP prints union members in its own rank order, not the declared one. The list built
    // above already collapses packed and keyed storage into one PHP type, so it only needs
    // ordering -- rebuilding it here would undo that de-duplication.
    sort_union_members_like_php(&mut types);
    (!types.is_empty()).then_some(ReflectionParameterTypeMetadata::Union(
        ReflectionUnionTypeMetadata { types, allows_null },
    ))
}

/// Converts a declared `A&B` type into `ReflectionIntersectionType` metadata.
pub(super) fn reflection_intersection_type_metadata(
    members: &[TypeExpr],
) -> Option<ReflectionParameterTypeMetadata> {
    let types = members
        .iter()
        .map(reflection_named_type_metadata_from_type_expr)
        .collect::<Option<Vec<_>>>()?;
    (!types.is_empty()).then_some(ReflectionParameterTypeMetadata::Intersection(
        ReflectionIntersectionTypeMetadata { types },
    ))
}

/// Converts one declared type atom into `ReflectionNamedType` metadata.
pub(super) fn reflection_named_type_metadata_from_type_expr(
    type_expr: &TypeExpr,
) -> Option<ReflectionNamedTypeMetadata> {
    match type_expr {
        TypeExpr::Int => Some(reflection_builtin_named_type("int", false)),
        TypeExpr::Float => Some(reflection_builtin_named_type("float", false)),
        TypeExpr::Bool => Some(reflection_builtin_named_type("bool", false)),
        TypeExpr::False => Some(reflection_builtin_named_type("false", false)),
        TypeExpr::Str => Some(reflection_builtin_named_type("string", false)),
        TypeExpr::Iterable => Some(reflection_builtin_named_type("iterable", false)),
        TypeExpr::Array(_) => Some(reflection_builtin_named_type("array", false)),
        TypeExpr::Named(name) => {
            let raw_name = name.as_str().trim_start_matches('\\');
            match raw_name.to_ascii_lowercase().as_str() {
                "array" | "callable" | "object" => {
                    Some(reflection_builtin_named_type(raw_name, false))
                }
                "mixed" => Some(reflection_builtin_named_type(raw_name, true)),
                _ => Some(ReflectionNamedTypeMetadata {
                    name: raw_name.to_string(),
                    allows_null: false,
                    is_builtin: false,
                }),
            }
        }
        _ => None,
    }
}

/// Orders a union's members the way PHP prints them, not the way they were declared.
///
/// PHP renders a union from its internal type mask, which has a fixed order, so `int|string`
/// prints as `string|int`. `ReflectionMethod::__toString()` already used this order (issue
/// #1080); the type OBJECT returned by `getType()` kept the declared one, so the same program
/// disagreed with itself:
///
/// ```text
/// echo (string) $p;             // Parameter #0 [ <required> string|int $v ]
/// echo (string) $p->getType();  // int|string
/// ```
///
/// The sort is stable, which is what keeps class names in their declared order among themselves:
/// they all share rank 0.
fn sort_union_members_like_php(types: &mut [ReflectionNamedTypeMetadata]) {
    types.sort_by_key(|member| {
        elephc_builtin_contract::reflection_format::union_member_rank(&member.name)
    });
}
