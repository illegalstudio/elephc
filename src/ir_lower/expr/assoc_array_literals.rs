//! Purpose:
//! Associative array literal typing and expression result inference.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.
//! - Construction and owned operands remain rooted across later expressions and hash writes.

use super::*;

/// Lowers an associative array literal.
pub(super) fn lower_assoc_array_literal(ctx: &mut LoweringContext<'_, '_>, pairs: &[(Expr, Expr)], expr: &Expr) -> LoweredValue {
    lower_assoc_array_literal_with_guard(ctx, pairs, expr, false)
}

/// Protects a callback's partially built argument hash until invocation consumes its ownership.
pub(super) fn lower_assoc_array_literal_with_guard(
    ctx: &mut LoweringContext<'_, '_>, pairs: &[(Expr, Expr)], expr: &Expr, guarded: bool,
) -> LoweredValue {
    let hash_ty = assoc_array_literal_type_for_ir(ctx, pairs, expr);
    lower_assoc_array_literal_as(ctx, pairs, expr, guarded, hash_ty)
}

/// Lowers an associative literal into an explicitly chosen hash storage type.
///
/// The storage type drives the per-value pointer retype, so a destination of pointer values
/// (`["k" => null]` bound to `array<string, ?ptr<T>>`) stores each `null` as the word 0.
pub(super) fn lower_assoc_array_literal_as(
    ctx: &mut LoweringContext<'_, '_>,
    pairs: &[(Expr, Expr)],
    expr: &Expr,
    guarded: bool,
    hash_ty: PhpType,
) -> LoweredValue {
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(pairs.len() as u32)),
        hash_ty.clone(),
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    let owner = publish_constructed_container(ctx, hash, expr.span);
    if guarded { guard_descriptor_container(ctx, hash, expr.span); }
    for (key, value) in pairs {
        let mut roots = Vec::new();
        let reference_source = match &value.kind {
            ExprKind::Reference(inner) => Some(inner.as_ref()),
            _ => None,
        };
        let value_expr = reference_source.unwrap_or(value);
        let key_value = lower_expr(ctx, key);
        let key = root_array_literal_item(ctx, key_value, key.span, &mut roots);
        let value_operand = lower_expr(ctx, value_expr);
        // A packed record address or `null` stored as a pointer value is the pointer word.
        let value_operand = match &hash_ty {
            PhpType::AssocArray { value: value_ty, .. } => {
                ctx.retype_packed_address_for_pointer_sink(value_operand, value_ty, Some(value.span))
            }
            _ => value_operand,
        };
        let value = root_array_literal_item(ctx, value_operand, value.span, &mut roots);
        let hash = load_published_container(ctx, owner, hash_ty.clone(), expr.span);
        ctx.emit_void(Op::HashSet, vec![hash.value, key.value, value.value], None, Op::HashSet.default_effects(), Some(expr.span));
        if let Some(source) = reference_source {
            let hash = load_published_container(ctx, owner, hash_ty.clone(), expr.span);
            let cell = ctx.emit_value(
                Op::LoadArrayElemRefCell,
                vec![hash.value, key.value],
                None,
                PhpType::Mixed,
                Op::LoadArrayElemRefCell.default_effects(),
                Some(value_expr.span),
            );
            bind_reference_literal_source(ctx, source, cell, PhpType::Mixed, value_expr.span);
        }
        if guarded { ctx.refresh_argument_array_guard(hash, expr.span); }
        for slot in roots.into_iter().rev() {
            retire_owned_call_operand(ctx, slot, expr.span);
        }
    }
    take_published_container(ctx, owner, hash_ty, expr.span)
}

/// Binds the variable a `&$name` literal element aliases to the entry's managed reference cell.
///
/// The entry was just inserted with the variable's current value and promoted to a reference by
/// `LoadArrayElemRefCell`. Rebinding the local to that same cell is what makes later writes on
/// either side visible through both, exactly like PHP's reference set.
pub(super) fn bind_reference_literal_source(
    ctx: &mut LoweringContext<'_, '_>,
    source: &Expr,
    cell: LoweredValue,
    value_type: PhpType,
    span: Span,
) {
    let ExprKind::Variable(name) = &source.kind else {
        // The parser restricts reference elements to variables, so a non-variable shape cannot
        // reach this helper; leave the entry as an ordinary value rather than miscompiling.
        return;
    };
    ctx.bind_owned_local_ref_cell_ptr(name, cell, value_type, Some(span));
}

/// Returns the associative-array type for a literal that contains at least one associative
/// spread. Mirrors the type checker's `assoc_spread_literal_value_type` so EIR storage matches
/// the value types actually lowered into the hash.
///
/// `spread_storage` holds, per item, the codegen storage `spread_source_storage_type_for_ir`
/// gave a spread's source (`None` for a plain item), typed once by the caller.
pub(super) fn assoc_array_literal_type_from_spreads(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
    expr: &Expr,
    spread_storage: &[Option<PhpType>],
) -> PhpType {
    // Every item is typed once for the pointer probe and the merge below, so an item or spread
    // source holding a nested merge is scanned once per nesting level. The merge types a spread
    // source as an element; that is what the source's storage already is for a merge or a
    // literal source, the sources that cost a scan per level, so those reuse the storage.
    let item_types: Vec<Option<PhpType>> = items
        .iter()
        .zip(spread_storage)
        .map(|(item, storage)| match (&item.kind, storage) {
            (ExprKind::Spread(_), Some(storage)) => Some(spread_storage_probe_element_type(storage)),
            (ExprKind::Spread(_), None) => None,
            _ => Some(array_literal_element_type_for_ir(ctx, item)),
        })
        .collect();
    // Pointers and `null`s (direct, or a spread of a null-only literal) share pointer storage.
    if let Some(pointer) = pointer_literal_element_type(ctx, items, &item_types) {
        return PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(pointer),
        };
    }
    let mut value_ty = PhpType::Never;
    for ((item, storage), item_ty) in items.iter().zip(spread_storage).zip(item_types) {
        let next = match (&item.kind, storage) {
            (ExprKind::Spread(inner), Some(storage)) if spread_source_is_typed_as_element(inner) => {
                spread_storage_probe_element_type(storage)
            }
            (ExprKind::Spread(inner), _) => {
                match array_literal_element_type_for_ir(ctx, inner).codegen_repr() {
                    PhpType::Array(elem) => elem.codegen_repr(),
                    PhpType::AssocArray { value, .. } => value.codegen_repr(),
                    _ => PhpType::Mixed,
                }
            }
            (_, _) => match item_ty {
                Some(ty) => ty.codegen_repr(),
                None => array_literal_element_type_for_ir(ctx, item).codegen_repr(),
            },
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

/// Returns true when `spread_source_storage_type_for_ir` types `source` exactly as
/// `array_literal_element_type_for_ir` does, so a merge may reuse the storage it already has.
///
/// The storage typer special-cases three kinds only: `@expr` (typed through its operand), a
/// function call (typed from its signature when one is known) and an array read (whose logical
/// null is stripped); every other source, a lazy merge, a nested literal, an `(array)` cast, a
/// variable or a property read, goes straight to the element typer.
fn spread_source_is_typed_as_element(source: &Expr) -> bool {
    !matches!(
        source.kind,
        ExprKind::ErrorSuppress(_) | ExprKind::FunctionCall { .. } | ExprKind::ArrayAccess { .. }
    )
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
    // The pointer probe below reuses a value's type where both typers agree (a lazy merge or a
    // nested literal, the values that cost a scan per nesting level); other values keep the
    // probe's own element typing, so its pointer verdict does not change.
    let mut probe_types: Vec<Option<PhpType>> = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        let next_key = normalized_array_key_type(key, infer_expr_type_syntactic(key));
        let next_value = assoc_array_literal_value_type_for_ir(ctx, value);
        probe_types.push(
            matches!(
                value.kind,
                ExprKind::Ternary { .. }
                    | ExprKind::ShortTernary { .. }
                    | ExprKind::Match { .. }
                    | ExprKind::NullCoalesce { .. }
                    | ExprKind::Assignment { .. }
                    | ExprKind::ArrayLiteral(_)
                    | ExprKind::ArrayLiteralAssoc(_)
            )
            .then(|| next_value.clone()),
        );
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
    let value_ty =
        pointer_literal_element_type(ctx, pairs.iter().map(|(_, value)| value), &probe_types)
            .unwrap_or(value_ty);
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
    if let Some(storage) = nullsafe_chain::result_storage_type(value) {
        return storage;
    }
    match &value.kind {
        // A reference entry's payload is a boxed Mixed cell owned by the managed reference, so
        // the hash storage stays dynamic regardless of the aliased place's declared type.
        ExprKind::Reference(_) => PhpType::Mixed,
        ExprKind::Null | ExprKind::Throw(_) => PhpType::Mixed,
        ExprKind::Cast {
            target: CastType::Array,
            expr: inner,
        } => array_cast_literal_element_type(ctx, inner),
        ExprKind::Ternary { .. }
        | ExprKind::ShortTernary { .. }
        | ExprKind::Match { .. }
        | ExprKind::NullCoalesce { .. }
        | ExprKind::Assignment { .. } => {
            // Use the actual branch-merge representation before choosing hash storage.
            ir_array_storage_type(materialized_expr_type_for_merge(ctx, value))
        }
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
        // A nested literal must be typed by the same context-aware function that will lower
        // it, not by the syntactic fallback: `infer_expr_type_syntactic` cannot see a local's
        // type, so it types every `$v` element `Int`. The outer hash then stamped its value
        // type `array<string, int>` over an inner hash really holding `array<string>`, and a
        // read through both levels returned the inner array pointer as an integer (issue
        // #984). These are the two arms `array_literal_element_type_for_ir` already carries
        // for an INDEXED outer literal, which is why `[["k" => $v]]` was always correct and
        // only `["j" => ["k" => $v]]` was wrong.
        ExprKind::ArrayLiteral(items) => {
            array_literal_type_for_ir(ctx, items, value).codegen_repr()
        }
        ExprKind::ArrayLiteralAssoc(inner_pairs) => {
            assoc_array_literal_type_for_ir(ctx, inner_pairs, value)
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
                return ir_array_storage_type(eir_user_function_return_type(sig));
            }
            if let Some(sig) = ctx.extern_functions.get(canonical) {
                return ir_array_storage_type(sig.return_type.clone());
            }
            // A builtin is in neither map, and the syntactic fallback below answers `Int` for
            // every name outside its hand-written allowlist -- which stamps the literal
            // `array<int>` and makes lowering cast the element to match (#1096). The checker
            // already asked the registry contract about this very call.
            if let Some(resolved) = builtin_call_result_type_for_ir(ctx, value.span) {
                return resolved;
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
        // A static buffer or pointer slot holds a raw word the syntactic `int` would cast.
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_word_type(ctx, receiver, property, value)
                .map(ir_array_storage_type)
                .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(value)))
        }
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
        // Keep the existing scalar classification, but resolve containers through the same
        // literal helpers that lower them. Syntactic inference reports Void for null and can
        // mistake nested scoped constants for strings inside an array initializer.
        return match &const_expr.kind {
            ExprKind::Null => PhpType::Mixed,
            ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, &const_expr),
            ExprKind::ArrayLiteralAssoc(pairs) => {
                assoc_array_literal_type_for_ir(ctx, pairs, &const_expr)
            }
            ExprKind::ArrayLiteralMixed(entries) => {
                assoc_array_literal_type_from_entries(ctx, entries, &const_expr)
            }
            _ => ir_array_storage_type(infer_expr_type_syntactic(&const_expr)),
        };
    }
    ir_array_storage_type(infer_expr_type_syntactic(value))
}

/// Returns the PHP value shape of an array access before merge or literal storage is selected.
///
/// A PHP array read can yield null on a missing key even when its elements are non-nullable.
/// Keep that logical null here; the read itself retains its existing typed fast path.
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
        // Any other receiver that loads a buffer (`make()[0]`, `($b ?? $c)[0]`) is typed from
        // what its own lowering loads; other receivers keep the previous syntactic answer.
        _ => buffer_receiver_type_for_ir(ctx, array),
    }?
    .codegen_repr();
    match array_ty {
        // A buffer element reads as its handle word, and a miss as the invalid handle 0 rather
        // than null, so the read keeps the plain buffer type: a nullable union would box the
        // handle in a merge or literal, where an element read of it answered null.
        PhpType::Array(element) | PhpType::AssocArray { value: element, .. }
            if matches!(element.codegen_repr(), PhpType::Buffer(_)) =>
        {
            Some(element.codegen_repr())
        }
        PhpType::Array(element) | PhpType::AssocArray { value: element, .. } => Some(
            nullable_aware_branch_merge_type(&normalize_value_php_type(*element), &PhpType::Void),
        ),
        PhpType::Str => Some(PhpType::Str),
        PhpType::Mixed | PhpType::Union(_) => Some(PhpType::Mixed),
        // A packed buffer element reads as its record address, which the checker types `ptr<T>`.
        // A scalar element keeps its own type: the syntactic fallback answers `int`, which
        // truncated a `buffer<float>` element in merges and literals.
        PhpType::Buffer(elem_ty) => match normalize_value_php_type(*elem_ty).codegen_repr() {
            PhpType::Packed(class_name) => Some(PhpType::Pointer(Some(class_name))),
            scalar @ (PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Pointer(_)) => {
                Some(scalar)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Returns the buffer type a receiver expression loads, or `None` when it is not a buffer.
///
/// A call, `??`, ternary, match or assignment answers through the merge predictor, which agrees
/// with the type their lowering materializes. Only a buffer answer is returned: the syntactic
/// fallback the caller keeps for everything else types an element read `int`, which truncated
/// `buffer<float>` elements read through these receivers.
fn buffer_receiver_type_for_ir(ctx: &LoweringContext<'_, '_>, receiver: &Expr) -> Option<PhpType> {
    let loaded = super::merge_temps::materialized_expr_type_for_merge(ctx, receiver);
    matches!(loaded.codegen_repr(), PhpType::Buffer(_)).then_some(loaded)
}

/// Returns the packed or extern class whose record an expression addresses, if the lowering
/// knows it.
///
/// Covers a packed buffer element, a nested packed field read, a `ptr<T>` to a packed record or
/// an extern struct returned by a call, and a local holding any of them.
fn record_class_of_expr(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> Option<String> {
    let ty = match &expr.kind {
        ExprKind::ArrayAccess { array, .. } => array_access_expr_value_type_for_ir(ctx, array),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
        }
        ExprKind::Variable(name) => ctx.local_types.get(name).cloned(),
        // A call returning `ptr<T>` to a packed record or extern struct (`first($b)->pos`).
        ExprKind::FunctionCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. } => {
            Some(super::merge_temps::materialized_expr_type_for_merge(ctx, expr))
        }
        _ => None,
    }?;
    let class_name = match ty.codegen_repr() {
        PhpType::Pointer(Some(class_name)) | PhpType::Packed(class_name) => class_name,
        _ => return None,
    };
    let normalized = class_name.trim_start_matches('\\');
    (ctx.packed_classes.contains_key(normalized) || ctx.extern_classes.contains_key(normalized))
        .then(|| normalized.to_string())
}

/// Returns the declared type for an object property expression used inside a literal.
pub(in crate::ir_lower) fn property_access_expr_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<PhpType> {
    // A packed or extern record field reads as its value type: a nested packed record as the
    // address of the embedded record (`ptr<T>` to the checker), a pointer field as its pointer,
    // and a scalar field as `int` (every integer width), `float` (`f32` too), `bool` or, for an
    // extern struct, `string`. The syntactic fallback answers `int` for every property read,
    // which truncated a float field in a literal or a merge, stored its raw bits under the int
    // tag in an associative literal, and read a string field as `0`.
    if let Some(record) = record_class_of_expr(ctx, object) {
        if let Some(info) = ctx.packed_classes.get(&record) {
            let field = info.fields.iter().find(|field| field.name == property)?;
            return match field.php_type.codegen_repr() {
                PhpType::Packed(inner) => Some(PhpType::Pointer(Some(inner))),
                value_ty => Some(value_ty),
            };
        }
        return ctx
            .extern_classes
            .get(&record)?
            .fields
            .iter()
            .find(|field| field.name == property)
            .map(|field| field.php_type.codegen_repr());
    }
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
    let method_key = php_symbol_key(method);
    let Some(class_name) = instance_callable_object_class(ctx, object) else {
        return dynamic_receiver_method_call_type_for_ir(ctx, object, &method_key);
    };
    class_method_signature(ctx, &class_name, &method_key)
        .map(|signature| normalize_value_php_type(crate::types::eir_value_php_type(&signature.return_type)))
}

/// Returns the result type of a call on a receiver of no single class (a `foreach` element of
/// an untyped array, a `mixed` value), as its lowering types it: dispatch on the runtime class
/// answers the signature every declaring class shares, or a boxed `mixed`. `None` for a receiver
/// that is not dynamic. Without it a literal holding `$block->component()` was stamped by the
/// syntactic `int` and the element was cast from the `Component` the call returned.
fn dynamic_receiver_method_call_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    method_key: &str,
) -> Option<PhpType> {
    let object_ty = super::merge_temps::materialized_expr_type_for_merge(ctx, object);
    if !super::method_metadata::dynamic_method_receiver_needs_mixed_fallback(&object_ty) {
        return None;
    }
    let shared = if ctx.has_eval_barrier() {
        None
    } else {
        super::method_metadata::common_dynamic_method_signature(ctx, method_key)
    };
    Some(shared.map_or(PhpType::Mixed, |signature| normalize_value_php_type(signature.return_type)))
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
