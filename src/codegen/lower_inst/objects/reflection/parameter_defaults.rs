//! Purpose:
//! Reflection parameter defaults and ReflectionType metadata conversion.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Converts a supported parameter default expression into Reflection metadata.
pub(super) fn reflection_parameter_default_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    default: &Expr,
) -> Result<Option<ReflectionParameterDefaultValue>> {
    if let Some(value) = reflection_global_default_value(default) {
        return Ok(Some(value));
    }
    if let Some(value) =
        reflection_object_parameter_default_value(ctx, current_class, current_info, default)?
    {
        return Ok(Some(value));
    }
    if let Some(value) = reflection_literal_parameter_default_value(default) {
        return Ok(Some(value));
    }
    match &default.kind {
        ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. } => {
            let value = reflection_constant_value(ctx, current_class, current_info, default, 0)?;
            Ok(reflection_parameter_default_from_constant_value(value))
        }
        _ => Ok(None),
    }
}

/// Converts a top-level object parameter default into Reflection metadata.
pub(super) fn reflection_object_parameter_default_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    default: &Expr,
) -> Result<Option<ReflectionParameterDefaultValue>> {
    let ExprKind::NewObject { class_name, args } = &default.kind else {
        return Ok(None);
    };
    let Some(args) = reflection_object_parameter_default_args(
        ctx,
        current_class,
        current_info,
        class_name.as_str(),
        args,
    )?
    else {
        return Ok(None);
    };
    Ok(Some(ReflectionParameterDefaultValue::Object {
        class_name: class_name.as_str().to_string(),
        args,
    }))
}

/// Returns constructor args for an object default, including supported omitted defaults.
pub(super) fn reflection_object_parameter_default_args(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    class_name: &str,
    args: &[Expr],
) -> Result<Option<Vec<ReflectionParameterDefaultValue>>> {
    if args.len() > 8 {
        return Ok(None);
    }
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        let Some(value) =
            reflection_parameter_default_non_object_value(ctx, current_class, current_info, arg)?
        else {
            return Ok(None);
        };
        values.push(value);
    }
    let Some((_, class_info)) = resolve_reflection_class(ctx, class_name) else {
        return Ok(Some(values));
    };
    let constructor = class_info.methods.get(&php_symbol_key("__construct"));
    let Some(constructor) = constructor else {
        return if values.is_empty() {
            Ok(Some(values))
        } else {
            Ok(None)
        };
    };
    if constructor.variadic.is_some()
        || values.len() > constructor.params.len()
        || constructor.params.len() > 8
    {
        return Ok(None);
    }
    for default in constructor.defaults.iter().skip(values.len()) {
        let Some(default) = default.as_ref() else {
            return Ok(None);
        };
        let Some(value) = reflection_parameter_default_non_object_value(
            ctx,
            class_name,
            Some(class_info),
            default,
        )?
        else {
            return Ok(None);
        };
        values.push(value);
    }
    if values.len() == constructor.params.len() {
        Ok(Some(values))
    } else {
        Ok(None)
    }
}

/// Converts a supported non-object parameter default expression into metadata.
pub(super) fn reflection_parameter_default_non_object_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    default: &Expr,
) -> Result<Option<ReflectionParameterDefaultValue>> {
    if let Some(value) = reflection_literal_parameter_default_non_object_value(default) {
        return Ok(Some(value));
    }
    match &default.kind {
        ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. } => {
            let value = reflection_constant_value(ctx, current_class, current_info, default, 0)?;
            Ok(reflection_parameter_default_from_constant_value(value))
        }
        _ => Ok(None),
    }
}

/// Converts constructor arguments for object defaults, rejecting nested objects.
pub(super) fn reflection_literal_parameter_default_non_object_value(
    default: &Expr,
) -> Option<ReflectionParameterDefaultValue> {
    let value = reflection_literal_parameter_default_value(default)?;
    if reflection_default_value_contains_object(&value) {
        return None;
    }
    Some(value)
}

/// Returns whether a retained default contains an object value.
pub(super) fn reflection_default_value_contains_object(value: &ReflectionParameterDefaultValue) -> bool {
    match value {
        ReflectionParameterDefaultValue::Object { .. } => true,
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
        | ReflectionParameterDefaultValue::Null => false,
    }
}

/// Converts a literal parameter/property default expression into Reflection metadata.
pub(super) fn reflection_literal_parameter_default_value(
    default: &Expr,
) -> Option<ReflectionParameterDefaultValue> {
    match &default.kind {
        ExprKind::IntLiteral(value) => Some(ReflectionParameterDefaultValue::Int(*value)),
        ExprKind::BoolLiteral(value) => Some(ReflectionParameterDefaultValue::Bool(*value)),
        ExprKind::FloatLiteral(value) => Some(ReflectionParameterDefaultValue::Float(*value)),
        ExprKind::StringLiteral(value) => Some(ReflectionParameterDefaultValue::Str(value.clone())),
        ExprKind::Null => Some(ReflectionParameterDefaultValue::Null),
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .map(reflection_literal_parameter_default_value)
            .collect::<Option<Vec<_>>>()
            .map(ReflectionParameterDefaultValue::Array),
        ExprKind::ArrayLiteralAssoc(entries) => reflection_assoc_array_default_value(entries),
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
) -> Option<ReflectionParameterDefaultValue> {
    entries
        .iter()
        .map(|(key, value)| {
            let key = reflection_default_array_key(key)?;
            let value = reflection_literal_parameter_default_value(value)?;
            Some(ReflectionDefaultAssocEntry { key, value })
        })
        .collect::<Option<Vec<_>>>()
        .map(ReflectionParameterDefaultValue::AssocArray)
}

/// Converts one supported associative-array key expression into PHP-normalized metadata.
pub(super) fn reflection_default_array_key(key: &Expr) -> Option<ReflectionDefaultArrayKey> {
    match &key.kind {
        ExprKind::IntLiteral(value) => Some(ReflectionDefaultArrayKey::Int(*value)),
        ExprKind::BoolLiteral(value) => Some(ReflectionDefaultArrayKey::Int(i64::from(*value))),
        ExprKind::FloatLiteral(value) => Some(ReflectionDefaultArrayKey::Int(*value as i64)),
        ExprKind::StringLiteral(value) => reflection_default_string_array_key(value),
        ExprKind::Null => Some(ReflectionDefaultArrayKey::Str(String::new())),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg().map(ReflectionDefaultArrayKey::Int),
            ExprKind::FloatLiteral(value) => Some(ReflectionDefaultArrayKey::Int((-*value) as i64)),
            _ => None,
        },
        _ => None,
    }
}

/// Normalizes a string array key according to PHP integer-string key rules.
pub(super) fn reflection_default_string_array_key(value: &str) -> Option<ReflectionDefaultArrayKey> {
    if is_php_integer_array_key(value) {
        value
            .parse::<i64>()
            .ok()
            .map(ReflectionDefaultArrayKey::Int)
    } else {
        Some(ReflectionDefaultArrayKey::Str(value.to_string()))
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
    }
}

/// Returns PHP's constant-name metadata for parameter defaults that name a class constant.
pub(super) fn reflection_parameter_default_constant_name(default: &Expr) -> Option<String> {
    match &default.kind {
        ExprKind::ScopedConstantAccess { receiver, name }
            if reflection_static_receiver_label(receiver)
                == "__ElephcReflectionGlobalConstant" =>
        {
            Some(name.clone())
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            Some(format!("{}::{}", reflection_static_receiver_label(receiver), name))
        }
        _ => None,
    }
}

/// Materializes one global-constant marker used only by reflected builtin defaults.
fn reflection_global_default_value(default: &Expr) -> Option<ReflectionParameterDefaultValue> {
    let ExprKind::ScopedConstantAccess { receiver, name } = &default.kind else {
        return None;
    };
    if reflection_static_receiver_label(receiver) != "__ElephcReflectionGlobalConstant" {
        return None;
    }
    match name.as_str() {
        "PHP_INT_MIN" => Some(ReflectionParameterDefaultValue::Int(i64::MIN)),
        "SUNFUNCS_RET_STRING" => Some(ReflectionParameterDefaultValue::Int(1)),
        _ => None,
    }
}

/// Returns the PHP source-visible receiver label for ReflectionParameter constant defaults.
pub(super) fn reflection_static_receiver_label(receiver: &StaticReceiver) -> String {
    match receiver {
        StaticReceiver::Named(name) => name.as_str().trim_start_matches('\\').to_string(),
        StaticReceiver::Self_ => "self".to_string(),
        StaticReceiver::Static => "static".to_string(),
        StaticReceiver::Parent => "parent".to_string(),
    }
}

/// Converts a normalized parameter type into a supported `ReflectionType` subset.
pub(super) fn reflection_parameter_type_metadata(
    type_expr: Option<&TypeExpr>,
    ty: &PhpType,
) -> Option<ReflectionParameterTypeMetadata> {
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
            let types = members
                .iter()
                .filter(|member| !matches!(member, TypeExpr::Void))
                .map(reflection_named_type_metadata_from_type_expr)
                .collect::<Option<Vec<_>>>()?;
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
        PhpType::Callable => Some(reflection_builtin_named_type("callable", false)),
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
    let non_null_members = members
        .iter()
        .filter(|member| !matches!(member, PhpType::Void))
        .collect::<Vec<_>>();
    if non_null_members.len() == 1 {
        let mut metadata = reflection_named_type_metadata(non_null_members[0])?;
        metadata.allows_null = allows_null;
        return Some(ReflectionParameterTypeMetadata::Named(metadata));
    }
    let types = non_null_members
        .into_iter()
        .map(reflection_named_type_metadata)
        .collect::<Option<Vec<_>>>()?;
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
