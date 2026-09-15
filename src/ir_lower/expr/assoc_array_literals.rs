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
        // A spread entry carries no key of its own: it merges its source into the hash at this
        // exact position, so php's overwrite order and integer-key renumbering both come out of
        // the ordinary sequential build rather than a separate merge.
        if let Some(inner) = crate::parser::ast::assoc_spread_source(key, value) {
            lower_assoc_spread_entry(ctx, hash, inner, key.span);
            continue;
        }
        let key = lower_expr(ctx, key);
        let value = lower_expr(ctx, value);
        ctx.emit_void(Op::HashSet, vec![hash.value, key.value, value.value], None, Op::HashSet.default_effects(), Some(expr.span));
    }
    hash
}

/// Merges one `...$source` entry of an associative array literal into the destination hash.
fn lower_assoc_spread_entry(
    ctx: &mut LoweringContext<'_, '_>,
    hash: LoweredValue,
    inner: &Expr,
    span: Span,
) {
    let source = lower_expr(ctx, inner);
    let source = if source.ir_type == IrType::Heap(IrHeapKind::Mixed) {
        super::indexed_array_literals::lower_boxed_array_spread_source(ctx, source, span)
    } else {
        source
    };
    super::indexed_array_literals::lower_hash_spread_into_hash_from_value(ctx, hash, source, span);
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
            ExprKind::Spread(inner) => match array_literal_element_type_for_ir(ctx, inner).codegen_repr() {
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
    // Seeded from the FIRST entry rather than from a neutral element: `merge_array_key_types`
    // answers Mixed for any two different types, so folding a placeholder into it would widen
    // every literal's key type.
    let mut key_ty: Option<PhpType> = None;
    let mut value_ty: Option<PhpType> = None;
    for (key, value) in pairs {
        // A spread entry contributes its SOURCE's key and value types, not the placeholder
        // pair that carries it.
        let (next_key, next_value) =
            match crate::parser::ast::assoc_spread_source(key, value) {
                Some(inner) => assoc_spread_entry_types(ctx, inner),
                None => (
                    normalized_array_key_type(key, infer_expr_type_syntactic(key)),
                    assoc_array_literal_value_type_for_ir(ctx, value),
                ),
            };
        key_ty = Some(match key_ty {
            Some(current) => merge_array_key_types(current, next_key),
            None => next_key,
        });
        value_ty = Some(match value_ty {
            Some(current) => merge_ir_assoc_value_type(current, next_value),
            None => next_value,
        });
    }
    let (Some(key_ty), Some(value_ty)) = (key_ty, value_ty) else {
        return fallback_expr_type(expr);
    };
    PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(value_ty),
    }
}

/// Returns the `(key, value)` storage types one spread entry contributes to the literal.
fn assoc_spread_entry_types(ctx: &LoweringContext<'_, '_>, inner: &Expr) -> (PhpType, PhpType) {
    match array_literal_element_type_for_ir(ctx, inner).codegen_repr() {
        PhpType::Array(elem) => (PhpType::Int, elem.codegen_repr()),
        PhpType::AssocArray { key, value } => (key.codegen_repr(), value.codegen_repr()),
        _ => (PhpType::Mixed, PhpType::Mixed),
    }
}

/// Returns the best EIR storage value type for one associative-array literal value.
pub(super) fn assoc_array_literal_value_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    value: &Expr,
) -> PhpType {
    if let Some(storage) = nullsafe_chain::result_storage_type(value) {
        return storage;
    }
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
            ir_array_storage_type(infer_expr_type_syntactic(value))
        }
        ExprKind::MethodCall { object, method, .. } => {
            method_call_expr_type_for_ir(ctx, object, method)
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
