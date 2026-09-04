//! Purpose:
//! Associative array literal typing and expression result inference.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers an associative array literal.
pub(super) fn lower_assoc_array_literal(ctx: &mut LoweringContext<'_, '_>, pairs: &[(Expr, Expr)], expr: &Expr) -> LoweredValue {
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(pairs.len() as u32)),
        assoc_array_literal_type_for_ir(ctx, pairs, expr),
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    for (key, value) in pairs {
        let key = lower_expr(ctx, key);
        let value = lower_expr(ctx, value);
        ctx.emit_void(Op::HashSet, vec![hash.value, key.value, value.value], None, Op::HashSet.default_effects(), Some(expr.span));
    }
    hash
}

/// Returns the associative-array type for a literal that contains at least one associative
/// spread. Mirrors the type checker's `assoc_spread_literal_value_type` so EIR storage matches
/// the value types actually lowered into the hash.
pub(super) fn assoc_array_literal_type_from_spreads(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
    expr: &Expr,
) -> PhpType {
    let mut value_ty = PhpType::Never;
    for item in items {
        let next = match &item.kind {
            ExprKind::Spread(inner) => match infer_expr_type_syntactic(inner).codegen_repr() {
                PhpType::Array(elem) => elem.codegen_repr(),
                PhpType::AssocArray { value, .. } => value.codegen_repr(),
                _ => PhpType::Mixed,
            },
            _ => array_literal_element_type_for_ir(ctx, item).codegen_repr(),
        };
        value_ty = merge_ir_assoc_value_type(value_ty, next);
    }
    if matches!(value_ty, PhpType::Never) {
        return fallback_expr_type(expr);
    }
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(value_ty),
    }
}

/// Returns the associative-array type that the EIR backend can faithfully materialize.
pub(super) fn assoc_array_literal_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    pairs: &[(Expr, Expr)],
    expr: &Expr,
) -> PhpType {
    if pairs.is_empty() {
        return fallback_expr_type(expr);
    }
    let mut key_ty = normalized_array_key_type(
        &pairs[0].0,
        infer_expr_type_syntactic(&pairs[0].0),
    );
    let mut value_ty = assoc_array_literal_value_type_for_ir(ctx, &pairs[0].1);
    for (key, value) in pairs.iter().skip(1) {
        key_ty = merge_array_key_types(
            key_ty,
            normalized_array_key_type(key, infer_expr_type_syntactic(key)),
        );
        value_ty = merge_ir_assoc_value_type(
            value_ty,
            assoc_array_literal_value_type_for_ir(ctx, value),
        );
    }
    PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(value_ty),
    }
}

/// Returns the best EIR storage value type for one associative-array literal value.
pub(super) fn assoc_array_literal_value_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    value: &Expr,
) -> PhpType {
    match &value.kind {
        ExprKind::Null => PhpType::Mixed,
        ExprKind::ConstRef(name) => ctx
            .constant_value(name.as_str())
            .map(|(_, ty)| ir_array_storage_type(ty))
            .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(value))),
        // A class constant or enum case must be typed the way `lower_scoped_constant`
        // resolves it, not by the syntactic `::class`-is-string default, or the hash
        // value-type stamp would diverge from the lowered value and corrupt reads.
        ExprKind::ScopedConstantAccess { receiver, name } => {
            scoped_constant_value_type_for_ir(ctx, receiver, name, value)
        }
        ExprKind::Variable(name) => ir_array_storage_type(
            ctx.local_types
                .get(name)
                .cloned()
                .unwrap_or_else(|| infer_expr_type_syntactic(value)),
        ),
        ExprKind::FunctionCall { name, .. } => {
            let canonical = name.as_str();
            if let Some(sig) = ctx.functions.get(canonical) {
                return ir_array_storage_type(sig.return_type.clone());
            }
            if let Some(sig) = ctx.extern_functions.get(canonical) {
                return ir_array_storage_type(sig.return_type.clone());
            }
            // A BUILTIN is neither of those, and the syntactic fallback cannot know one: it
            // answered `Str` for `json_decode()`, so the whole literal became `array<_, string>`
            // and every value read back as a declared string. The checker already decided this
            // call's type and keyed it by span — the indexed walk asks the same question.
            if let Some(ty) = ctx.builtin_call_types.get(&value.span) {
                return ir_array_storage_type(ty.clone());
            }
            ir_array_storage_type(infer_expr_type_syntactic(value))
        }
        ExprKind::MethodCall { object, method, .. } => {
            method_call_expr_type_for_ir(ctx, object, method)
                .and_then(materializable_array_element_type)
                .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(value)))
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            nullsafe_method_call_expr_type_for_ir(ctx, object, method)
                .and_then(materializable_array_element_type)
                .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(value)))
        }
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            static_method_call_expr_type_for_ir(ctx, receiver, method)
                .and_then(materializable_array_element_type)
                .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(value)))
        }
        ExprKind::ArrayAccess { array, .. } => array_access_expr_value_type_for_ir(ctx, array)
            .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(value))),
        ExprKind::PropertyAccess { object, property } => property_access_expr_type_for_ir(
            ctx,
            object,
            property,
        )
        .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(value))),
        _ => ir_array_storage_type(infer_expr_type_syntactic(value)),
    }
}

/// Returns the EIR storage value type for a scoped-constant array value,
/// resolving a class/interface constant the same way `lower_scoped_constant`
/// lowers it so the hash value-type stamp matches the value actually stored
/// (rather than the syntactic `::class`-is-string default). Falls back to the
/// syntactic guess when the constant cannot be resolved.
pub(super) fn scoped_constant_value_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    member: &str,
    value: &Expr,
) -> PhpType {
    let class_name = scoped_constant_receiver_name(ctx, receiver);
    let normalized = class_name.trim_start_matches('\\');
    // An enum case lowers to the case *object* singleton (see `lower_scoped_constant`),
    // so the hash must box it as a Mixed cell — stamp the value type Mixed to match.
    if ctx
        .enums
        .get(normalized)
        .is_some_and(|enum_info| enum_info.cases.iter().any(|case| case.name == member))
    {
        return PhpType::Mixed;
    }
    if let Some(const_expr) = ctx.scoped_constant_value(&class_name, member) {
        return ir_array_storage_type(infer_expr_type_syntactic(&const_expr));
    }
    ir_array_storage_type(infer_expr_type_syntactic(value))
}

/// Returns the element/value type for an array-access expression used inside a literal.
pub(in crate::ir_lower) fn array_access_expr_value_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> Option<PhpType> {
    let array_ty = match &array.kind {
        ExprKind::Variable(name) => ctx.local_types.get(name).cloned(),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
        }
        ExprKind::ArrayLiteral(items) => Some(array_literal_type_for_ir(ctx, items, array)),
        ExprKind::ArrayLiteralAssoc(pairs) => Some(assoc_array_literal_type_for_ir(ctx, pairs, array)),
        // A call result is subscripted directly often enough to matter — `meta()["mode"]`. Without
        // this arm the caller fell back to `infer_expr_type_syntactic`, whose last resort for an
        // unknown call is `int`, so a ternary merging that read with `false` typed its temp `int`
        // and CAST the string element to one: `$c ? false : meta()["mode"]` printed `0`.
        ExprKind::FunctionCall { name, .. } => Some(call_return_type(ctx, name, &[])),
        _ => None,
    }?
    .codegen_repr();
    match array_ty {
        PhpType::Array(elem_ty) => {
            Some(array_access_element_result_type(normalize_value_php_type(*elem_ty).codegen_repr()))
        }
        PhpType::AssocArray { value, .. } => {
            Some(array_access_element_result_type(normalize_value_php_type(*value).codegen_repr()))
        }
        PhpType::Str => Some(PhpType::Str),
        PhpType::Mixed | PhpType::Union(_) => Some(PhpType::Mixed),
        _ => None,
    }
}

/// Returns the declared type for an object property expression used inside a literal.
pub(in crate::ir_lower) fn property_access_expr_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<PhpType> {
    let class_name = instance_callable_object_class(ctx, object)?;
    let normalized = class_name.trim_start_matches('\\');
    if is_builtin_stdclass_name(normalized) {
        return Some(PhpType::Mixed);
    }
    if let Some(property_ty) = runtime_property_type_override(ctx, normalized, property) {
        return Some(normalize_value_php_type(property_ty));
    }
    let class_info = ctx.classes.get(normalized)?;
    class_info
        .properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| normalize_value_php_type(ty.codegen_repr()))
}

/// Returns the declared property result type plus `null` when a nullsafe receiver may be null.
pub(super) fn nullsafe_property_access_expr_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<PhpType> {
    let property_type = property_access_expr_type_for_ir(ctx, object, property)?;
    let (_, nullable) = instance_callable_object_class_and_nullability(ctx, object)?;
    if nullable {
        Some(nullable_result_type(property_type))
    } else {
        Some(property_type)
    }
}

/// Returns the declared result type for an instance method call before its receiver is lowered.
pub(in crate::ir_lower) fn method_call_expr_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    method: &str,
) -> Option<PhpType> {
    let class_name = instance_callable_object_class(ctx, object)?;
    let method_key = php_symbol_key(method);
    class_method_signature(ctx, &class_name, &method_key)
        .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))
}

/// Returns the declared method result type plus `null` when a nullsafe receiver may be null.
pub(super) fn nullsafe_method_call_expr_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    method: &str,
) -> Option<PhpType> {
    let return_type = method_call_expr_type_for_ir(ctx, object, method)?;
    let (_, nullable) = instance_callable_object_class_and_nullability(ctx, object)?;
    if nullable {
        Some(nullable_result_type(return_type))
    } else {
        Some(return_type)
    }
}

/// Merges associative-array value types for EIR storage metadata.
pub(crate) fn merge_ir_assoc_value_type(left: PhpType, right: PhpType) -> PhpType {
    ir_array_storage_type(PhpType::widen_array_branch_element(left, right))
}

