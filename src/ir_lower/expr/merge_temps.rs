//! Purpose:
//! Branch merge temporaries, container widening, and fallback typing.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Stores a lowered expression result into a hidden merge temporary.
pub(super) fn store_expr_into_temp(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    temp_type: PhpType,
    expr: &Expr,
    span: crate::span::Span,
) {
    // A `throw` arm of a buffer merge leaves through its exception and stores nothing: the boxed
    // null its lowering answers is not a buffer handle, so the branch ends where the throw does
    // and the merge block only joins the arms that store a buffer.
    if matches!(expr.kind, ExprKind::Throw(_))
        && matches!(temp_type.codegen_repr(), PhpType::Buffer(_))
    {
        lower_expr(ctx, expr);
        ctx.builder.terminate(Terminator::Unreachable);
        return;
    }
    // A pointer-array temporary builds a literal branch as pointer words, so `[null]` is the
    // null pointer rather than a boxed null the merged array cannot hold.
    if super::indexed_array_literals::literal_pointer_element_type(&temp_type).is_some()
        && matches!(expr.kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_))
    {
        let value = lower_expr_for_destination(ctx, expr, &temp_type);
        store_value_into_temp(ctx, temp_name, temp_type, value, span);
        return;
    }
    if temp_type.codegen_repr() == PhpType::Mixed {
        if let Some(value) = lower_array_value_for_merge(ctx, expr) {
            store_value_into_temp(ctx, temp_name, temp_type, value, span);
            return;
        }
    }
    let value = lower_expr(ctx, expr);
    store_value_into_temp(ctx, temp_name, temp_type, value, span);
}

/// Stores an already lowered value into a hidden merge temporary.
pub(super) fn store_value_into_temp(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    temp_type: PhpType,
    value: LoweredValue,
    span: crate::span::Span,
) {
    let value = coerce_value_for_temp(ctx, value, &temp_type, span);
    let source = value;
    let stored = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.store_local(temp_name, stored, temp_type, Some(span));
    if stored.value != source.value && ctx.value_needs_release_after_retaining_store(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
}

/// Loads an owned hidden temp into SSA and clears the backing slot without releasing it.
pub(super) fn take_owned_temp(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    span: crate::span::Span,
) -> LoweredValue {
    let value = ctx.load_local(temp_name, Some(span));
    ctx.clear_owned_hidden_temp(temp_name, Some(span));
    value
}

/// Types one arm of a lazy merge the way its own lowering will see it: an array or hash literal
/// through the context-aware literal typer, anything else through
/// `materialized_expr_type_for_merge`. Called once per arm, so a merge nested in a literal arm is
/// scanned once per level.
fn merge_arm_type(ctx: &LoweringContext<'_, '_>, arm: &Expr) -> PhpType {
    match &arm.kind {
        ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, arm),
        ExprKind::ArrayLiteralAssoc(pairs) => {
            super::assoc_array_literals::assoc_array_literal_type_for_ir(ctx, pairs, arm)
        }
        _ => materialized_expr_type_for_merge(ctx, arm),
    }
}

/// The type an arm contributes to the ordinary merge: a literal of buffers keeps its contextual
/// type, every other literal the syntactic fallback it always had, and any other arm the
/// materialized type `merge_arm_type` found.
fn generic_arm_type(arm: &Expr, contextual: &PhpType) -> PhpType {
    if matches!(arm.kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) {
        buffer_container_or_fallback(contextual.clone(), arm)
    } else {
        contextual.clone()
    }
}

/// Keeps `ty` when it is an array or hash of buffer handles, otherwise answers the syntactic
/// fallback type of `expr`, as every other literal is typed for a merge.
fn buffer_container_or_fallback(ty: PhpType, expr: &Expr) -> PhpType {
    let is_buffer_container = super::indexed_array_literals::literal_raw_word_element_type(&ty)
        .is_some_and(|elem| matches!(elem, PhpType::Buffer(_)));
    if is_buffer_container {
        ty
    } else {
        fallback_expr_type(expr)
    }
}

/// Returns the `buffer<T>` type every arm that completes shares, if they all hold the same one.
///
/// A buffer is an opaque handle word that has no `mixed` form: the ordinary merge widened two
/// `buffer<T>` arms of a ternary or `match` against the syntactic fallback (`int`) into `mixed`,
/// boxing the handle as an integer that an element read then answered with null. A `throw` arm
/// stores nothing (`store_expr_into_temp` ends its branch), so it does not widen the merge. Any
/// other mix keeps the ordinary merge; the checker rejects a buffer arm beside another type.
/// The arms come typed, so nothing is typed again here.
fn shared_buffer_merge_type(arms: &[(&Expr, PhpType)]) -> Option<PhpType> {
    let mut shared: Option<PhpType> = None;
    for (arm, ty) in arms {
        if matches!(arm.kind, ExprKind::Throw(_)) {
            continue;
        }
        let ty = ty.codegen_repr();
        if !matches!(ty, PhpType::Buffer(_)) {
            return None;
        }
        match &shared {
            Some(existing) if *existing != ty => return None,
            Some(_) => {}
            None => shared = Some(ty),
        }
    }
    shared
}

/// Chooses a merge temp type from contextual branch materialization and fallback metadata.
pub(super) fn branch_merge_result_type(
    ctx: &LoweringContext<'_, '_>,
    then_expr: &Expr,
    else_expr: &Expr,
    expr: &Expr,
) -> PhpType {
    let arms = [(then_expr, merge_arm_type(ctx, then_expr)), (else_expr, merge_arm_type(ctx, else_expr))];
    if let Some(raw_words) = pointer_array_branch_merge_type(&arms) {
        return raw_words;
    }
    if let Some(buffer) = shared_buffer_merge_type(&arms) {
        return buffer;
    }
    let then_ty = generic_arm_type(then_expr, &arms[0].1);
    let else_ty = generic_arm_type(else_expr, &arms[1].1);
    let branch_ty = nullable_aware_branch_merge_type(&then_ty, &else_ty);
    // A pointer merge (a `ptr<T>` or packed record address on both sides) is already exact; the
    // syntactic fallback would answer `int` for the ternary and widen the word into `Mixed`.
    if php_type_allows_null(&branch_ty)
        || matches!(branch_ty.codegen_repr(), PhpType::Pointer(_) | PhpType::Packed(_))
    {
        return branch_ty;
    }
    let fallback_ty = fallback_expr_type(expr).codegen_repr();
    wider_type_for_merge(&fallback_ty, &branch_ty.codegen_repr())
}

/// Chooses a match hidden-temp type by merging every arm result type, so
/// heterogeneous arms (e.g. object/array/string) materialize a Mixed temp
/// boxed per arm instead of coercing all arms to one unified scalar type.
pub(super) fn match_merge_result_type(
    ctx: &LoweringContext<'_, '_>,
    arms: &[(Vec<Expr>, Expr)],
    default: Option<&Expr>,
    expr: &Expr,
) -> PhpType {
    let typed_arms: Vec<(&Expr, PhpType)> = arms
        .iter()
        .map(|(_, result)| result)
        .chain(default)
        .map(|result| (result, merge_arm_type(ctx, result)))
        .collect();
    if let Some(raw_words) = pointer_array_branch_merge_type(&typed_arms) {
        return raw_words;
    }
    if let Some(buffer) = shared_buffer_merge_type(&typed_arms) {
        return buffer;
    }
    let mut merged: Option<PhpType> = None;
    for (result, contextual) in &typed_arms {
        let arm_ty = generic_arm_type(result, contextual);
        merged = Some(match merged {
            Some(acc) => nullable_aware_branch_merge_type(&acc, &arm_ty),
            None => arm_ty,
        });
    }
    let Some(merged) = merged else {
        return fallback_expr_type(expr);
    };
    // As for a ternary, an exact pointer merge must not be widened by the syntactic fallback.
    if php_type_allows_null(&merged)
        || matches!(merged.codegen_repr(), PhpType::Pointer(_) | PhpType::Packed(_))
    {
        return merged;
    }
    let fallback_ty = fallback_expr_type(expr).codegen_repr();
    wider_type_for_merge(&fallback_ty, &merged.codegen_repr())
}

/// Chooses a short-ternary hidden-temp type without reintroducing the
/// scalar-biased syntactic join used by the parser-only fallback inference.
pub(super) fn short_ternary_merge_result_type(
    ctx: &LoweringContext<'_, '_>,
    value: &Expr,
    default: &Expr,
) -> PhpType {
    let arms = [(value, merge_arm_type(ctx, value)), (default, merge_arm_type(ctx, default))];
    if let Some(raw_words) = pointer_array_branch_merge_type(&arms) {
        return raw_words;
    }
    // The tested value is stored as it was lowered, so only the default may be a `throw` arm.
    if !matches!(value.kind, ExprKind::Throw(_)) {
        if let Some(buffer) = shared_buffer_merge_type(&arms) {
            return buffer;
        }
    }
    let value_ty = generic_arm_type(value, &arms[0].1);
    let default_ty = generic_arm_type(default, &arms[1].1);
    // A `?ptr<T>` side keeps the merge nullable: its codegen form is the bare pointer word, and
    // joining that would type a null result as a set pointer.
    if crate::types::is_nullable_pointer_type(&value_ty)
        || crate::types::is_nullable_pointer_type(&default_ty)
    {
        return nullable_aware_branch_merge_type(&value_ty, &default_ty);
    }
    materialized_merge_type(&value_ty.codegen_repr(), &default_ty.codegen_repr())
}

/// Chooses a ternary branch merge type without erasing PHP null branches.
pub(super) fn nullable_aware_branch_merge_type(left: &PhpType, right: &PhpType) -> PhpType {
    if php_type_allows_null(left) || php_type_allows_null(right) {
        let left_non_null = strip_void_from_union(left.clone());
        let right_non_null = strip_void_from_union(right.clone());
        return normalize_union_members(vec![PhpType::Void, left_non_null, right_non_null])
            .unwrap_or(PhpType::Void);
    }
    materialized_merge_type(left, right)
}

/// Returns the pointer or buffer array (or hash) storage that merged branches share, if any.
///
/// Every branch must be a pointer array (or hash) value or a literal of only `null`s, with at
/// least one of them holding pointers and all of them the same shape. A `[null]` branch types
/// as `array<mixed>` on its own, so the ordinary merge would box the pointer branches, while the
/// checker joins `ptr<T>` and `null` into one pointer element.
///
/// Arrays of buffer handles are raw words too: every branch must then hold the same `buffer<T>`
/// (a null literal cannot be a buffer element), so `$a ?: [$b]` keeps the handles as words
/// instead of boxing both branches into `array<mixed>` and reading the cells back as handles.
///
/// Each arm comes with the type `merge_arm_type` gave it, so the arms are typed once whether
/// the raw-word storage or the ordinary merge wins: typing them here again would scan a nested
/// merge twice per level.
pub(super) fn pointer_array_branch_merge_type(arms: &[(&Expr, PhpType)]) -> Option<PhpType> {
    let mut merged: Option<PhpType> = None;
    let mut null_literal_shapes = Vec::new();
    let mut holds_buffers = false;
    let mut empty_literal_shapes = Vec::new();
    for (branch, contextual) in arms {
        if super::indexed_array_literals::is_null_only_literal(branch) {
            null_literal_shapes.push(matches!(branch.kind, ExprKind::ArrayLiteralAssoc(_)));
            continue;
        }
        // A literal branch is typed the way its own lowering stamps it.
        let ty = contextual.codegen_repr();
        let Some(elem) = super::indexed_array_literals::literal_raw_word_element_type(&ty) else {
            // `[]` holds no word to type; it only has to match the merged shape.
            if super::indexed_array_literals::is_empty_literal(branch) {
                empty_literal_shapes.push(matches!(branch.kind, ExprKind::ArrayLiteralAssoc(_)));
                continue;
            }
            return None;
        };
        holds_buffers |= matches!(elem, PhpType::Buffer(_));
        merged = Some(match merged {
            None => ty,
            Some(acc) => merge_pointer_containers(acc, ty)?,
        });
    }
    let merged = merged?;
    if holds_buffers {
        // Differing buffer types (or a buffer beside a pointer) widen the element to `mixed`,
        // and a null literal has no buffer word: both keep the ordinary merge.
        let same_buffer = super::indexed_array_literals::literal_raw_word_element_type(&merged)
            .is_some_and(|elem| matches!(elem, PhpType::Buffer(_)));
        if !same_buffer || !null_literal_shapes.is_empty() {
            return None;
        }
    } else if !empty_literal_shapes.is_empty() {
        // A pointer merge keeps its ordinary path for `[]`: the boxed result is rewritten into
        // pointer words by the destination conversions.
        return None;
    }
    let merged_is_hash = matches!(merged, PhpType::AssocArray { .. });
    null_literal_shapes
        .into_iter()
        .chain(empty_literal_shapes)
        .all(|is_hash| is_hash == merged_is_hash)
        .then_some(merged)
}

/// Joins two pointer arrays or two pointer hashes into the storage both fit.
pub(super) fn merge_pointer_containers(left: PhpType, right: PhpType) -> Option<PhpType> {
    match (left, right) {
        (PhpType::Array(left), PhpType::Array(right)) => Some(PhpType::Array(Box::new(
            PhpType::widen_array_branch_element(*left, *right).codegen_repr(),
        ))),
        (
            PhpType::AssocArray { key: left_key, value: left_value },
            PhpType::AssocArray { key: right_key, value: right_value },
        ) => Some(PhpType::AssocArray {
            key: Box::new(if left_key == right_key { *left_key } else { PhpType::Mixed }),
            value: Box::new(
                PhpType::widen_array_branch_element(*left_value, *right_value).codegen_repr(),
            ),
        }),
        _ => None,
    }
}

/// Returns true when a PHP type can materialize PHP null at runtime.
pub(super) fn php_type_allows_null(php_type: &PhpType) -> bool {
    match php_type {
        PhpType::Void | PhpType::Never | PhpType::Mixed => true,
        PhpType::Union(members) => members
            .iter()
            .any(|member| matches!(member, PhpType::Void | PhpType::Never | PhpType::Mixed)),
        _ => false,
    }
}

/// Estimates the value type an expression will materialize during branch lowering.
pub(super) fn materialized_expr_type_for_merge(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> PhpType {
    // A nullsafe postfix chain lowers to its boxed chain temp whatever its last segment reads,
    // so `$o?->floats[0]` must not be predicted as the syntactic `int` and cast back.
    if let Some(storage) = super::nullsafe_chain::result_storage_type(expr) {
        return storage;
    }
    match &expr.kind {
        ExprKind::Variable(name) => {
            normalize_value_php_type(crate::types::eir_value_php_type(&ctx.local_type(name)))
        }
        ExprKind::ErrorSuppress(inner) => materialized_expr_type_for_merge(ctx, inner),
        ExprKind::BinaryOp { left, op, right } if mixed_numeric_op(op).is_some() => {
            let left_ty = materialized_expr_type_for_merge(ctx, left).codegen_repr();
            let right_ty = materialized_expr_type_for_merge(ctx, right).codegen_repr();
            if matches!(left_ty, PhpType::Mixed | PhpType::Union(_))
                || matches!(right_ty, PhpType::Mixed | PhpType::Union(_))
            {
                PhpType::Mixed
            } else {
                fallback_expr_type(expr)
            }
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => branch_merge_result_type(ctx, then_expr, else_expr, expr),
        ExprKind::Match { arms, default, .. } => {
            match_merge_result_type(ctx, arms, default.as_deref(), expr)
        }
        ExprKind::ShortTernary { value, default } => {
            short_ternary_merge_result_type(ctx, value, default)
        }
        ExprKind::NullCoalesce { value, default } => {
            null_coalesce_merge_type(ctx, null_coalesce_value_type_for_merge(ctx, value), default)
        }
        ExprKind::Assignment {
            value,
            result_target,
            prelude,
            ..
        } => materialized_expr_type_for_merge(
            ctx,
            assignment_result_expr(value, result_target.as_deref(), prelude),
        ),
        ExprKind::ArrayAccess { array, .. } => array_access_expr_value_type_for_ir(ctx, array)
            .unwrap_or_else(|| fallback_expr_type(expr)),
        // Typed the way `lower_buffer_new` stamps the allocation, so `buffer_new<float>(1)[0]`
        // is read as a float element rather than the syntactic `int`.
        ExprKind::BufferNew { element_type, .. } => {
            PhpType::Buffer(Box::new(ctx.type_expr_to_php_type_for_value(element_type)))
        }
        ExprKind::PropertyAccess { object, property } => {
            nullable_pointer_property_type(ctx, object, property)
                .or_else(|| property_access_expr_type_for_ir(ctx, object, property))
                .unwrap_or_else(|| fallback_expr_type(expr))
        }
        // Only a `?ptr<T>` static property is resolved here, so its null member survives the
        // merge; other static reads keep the syntactic answer.
        ExprKind::StaticPropertyAccess { receiver, property } => {
            let loaded = static_property_result_type(ctx, receiver, property, expr);
            if crate::types::is_nullable_pointer_type(&loaded) {
                loaded
            } else {
                fallback_expr_type(expr)
            }
        }
        // Calls are typed from the return metadata their own lowering uses. The syntactic
        // fallback answers `int` for every call outside its builtin allowlist, and a merge temp
        // typed from that guess coerced a string, array or object branch into an integer slot or,
        // beside a `null` branch, into a tagged `?int` scalar the backend cannot store an array
        // in (issue #1364). A call whose result cannot be named here merges as boxed `mixed`,
        // which holds whatever the call returns.
        ExprKind::FunctionCall { name, .. } => function_call_type_for_merge(ctx, name.as_str(), expr),
        ExprKind::MethodCall { object, method, .. } => {
            method_call_expr_type_for_ir(ctx, object, method).unwrap_or(PhpType::Mixed)
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            nullsafe_method_call_expr_type_for_ir(ctx, object, method).unwrap_or(PhpType::Mixed)
        }
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            static_method_call_expr_type_for_ir(ctx, receiver, method).unwrap_or(PhpType::Mixed)
        }
        ExprKind::ClosureCall { var, .. } => closure_call_type_for_merge(ctx, var),
        ExprKind::ExprCall { .. } => PhpType::Mixed,
        // A literal of buffers keeps its context-aware type: the syntactic fallback would type
        // the handles as integers, and a merge with `null` or a `??` default would then box the
        // array into cells that the buffer readers take for handles.
        ExprKind::ArrayLiteral(items) => {
            buffer_container_or_fallback(array_literal_type_for_ir(ctx, items, expr), expr)
        }
        ExprKind::ArrayLiteralAssoc(pairs) => buffer_container_or_fallback(
            super::assoc_array_literals::assoc_array_literal_type_for_ir(ctx, pairs, expr),
            expr,
        ),
        _ => fallback_expr_type(expr),
    }
}

/// Returns the expression an assignment expression evaluates to, for type prediction.
///
/// That is its result target, or the assigned value when there is none. A non-local assignment
/// binds a value it cannot replay into a generated local in its prelude and reads that local
/// back; a merge or literal is typed before the prelude runs, when the local is still unknown,
/// so the local resolves to the value the prelude binds into it.
pub(super) fn assignment_result_expr<'e>(
    value: &'e Expr,
    result_target: Option<&'e Expr>,
    prelude: &'e [Stmt],
) -> &'e Expr {
    let result = result_target.unwrap_or(value);
    let ExprKind::Variable(name) = &result.kind else {
        return result;
    };
    prelude
        .iter()
        .rev()
        .find_map(|stmt| match &stmt.kind {
            StmtKind::Assign { name: bound, value } if bound == name => Some(value),
            _ => None,
        })
        .unwrap_or(result)
}

/// Estimates the type the value side of `??` materializes before its null arm is removed.
///
/// A local keeps its declared PHP type here: `?int` stays `int|null`, so stripping the null
/// leaves `int`, the same answer `null_coalesce_result_type` reads from the lowered load. The
/// codegen representation (`TaggedScalar`) would hide the int behind the nullable carrier.
fn null_coalesce_value_type_for_merge(ctx: &LoweringContext<'_, '_>, value: &Expr) -> PhpType {
    match &value.kind {
        ExprKind::Variable(name) => normalize_value_php_type(ctx.local_type(name)),
        _ => materialized_expr_type_for_merge(ctx, value),
    }
}

/// Returns the declared type of an instance property read when it is a `?ptr<T>`.
///
/// `property_access_expr_type_for_ir` answers the codegen form, the bare pointer, which is what
/// literal storage wants but would drop the null member from a merge.
fn nullable_pointer_property_type(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<PhpType> {
    let class_name = instance_callable_object_class(ctx, object)?;
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    class_info
        .properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
        .filter(crate::types::is_nullable_pointer_type)
}

/// Returns the merge type of a named function call before it is lowered.
///
/// A user or extern function answers the return type `lower_function_call` stamps on the call.
/// A pop or shift uses its shared EIR result contract, which is boxed `Mixed` even when
/// the checker sees a scalar array element: removing from an empty array returns null.
/// Other builtins keep the syntactic answer when that names a type, and otherwise consult
/// the checker's result type for this very call (`int`, `bool`, `float` and `string` stay
/// scalar, anything else is boxed), because syntactic `int` is only a default outside its
/// allowlist.
fn function_call_type_for_merge(ctx: &LoweringContext<'_, '_>, canonical: &str, expr: &Expr) -> PhpType {
    if !source_prefers_extension_builtin(canonical) {
        if let Some(signature) = ctx.functions.get(canonical) {
            return normalize_value_php_type(eir_user_function_return_type(signature));
        }
    }
    if let Some(signature) = ctx.extern_functions.get(canonical) {
        return normalize_value_php_type(signature.return_type.clone());
    }
    if let Some(def) = crate::builtins::registry::lookup(canonical.trim_start_matches('\\')) {
        if matches!(
            def.spec.semantics.lowering,
            crate::builtins::semantics::BuiltinLowering::Runtime(
                crate::ir::RuntimeCallTarget::Function(
                    crate::ir::RuntimeFnId::ArrayPop | crate::ir::RuntimeFnId::ArrayShift
                )
            )
        ) {
            return resolve_registry_builtin_result_type(
                ctx,
                def.name,
                &[],
                &[],
                expr.span,
                None,
            )
            .expect("registered pop/shift builtin must resolve its result type");
        }
    }
    let syntactic = fallback_expr_type(expr);
    if syntactic != PhpType::Int {
        return syntactic;
    }
    builtin_call_result_type_for_ir(ctx, expr.span).unwrap_or(syntactic)
}

/// Returns the merge type of a `$callable(...)` call through a local variable.
///
/// Mirrors `lower_closure_call`: a statically known closure or user function answers its
/// signature's return type and a typed callable parameter its descriptor signature's.
/// Everything else (builtin names, methods, invokable objects, unknown callables) merges as
/// boxed `mixed`.
fn closure_call_type_for_merge(ctx: &LoweringContext<'_, '_>, var: &str) -> PhpType {
    match ctx.static_callable_local(var) {
        Some(target @ StaticCallableBinding::Closure { .. })
        | Some(target @ StaticCallableBinding::UserFunction(_)) => {
            return static_callable_return_type(ctx, &target);
        }
        Some(_) => return PhpType::Mixed,
        None => {}
    }
    ctx.callable_param_signature(var)
        .map(|signature| descriptor_invoker_result_type(Some(signature)))
        .unwrap_or(PhpType::Mixed)
}

/// Coerces branch values to the hidden temp storage type before storing them.
pub(super) fn coerce_value_for_temp(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    temp_type: &PhpType,
    span: crate::span::Span,
) -> LoweredValue {
    let target_ty = temp_type.codegen_repr();
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if source_ty == target_ty {
        return value;
    }
    match &target_ty {
        PhpType::Mixed => ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span)),
        PhpType::Pointer(_)
            if matches!(source_ty, PhpType::Packed(_) | PhpType::Void | PhpType::Never) =>
        {
            ctx.retype_packed_address_for_pointer_sink(value, &target_ty, Some(span))
        }
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never => {
            coerce_to_int_at_span(ctx, value, Some(span))
        }
        PhpType::Float => coerce_to_float_at_span(ctx, value, Some(span)),
        PhpType::Str => coerce_to_string_at_span(ctx, value, Some(span)),
        // A boxed `?C` (a nullable parameter or property) merged into a `C` temp: `$o ??= new C()`
        // types the merge from the non-null side, so the arm that keeps `$o` must hand over the
        // object inside the cell. Storing the cell pointer itself made the next member access
        // read the box as the object (#1628). The unbox owns its own lease of the payload.
        // `MixedUnbox` does not check the tag, so this arm relies on never seeing null: the
        // only producer of an object temp from a nullable value is `null_coalesce_result_type`,
        // and its value arm runs behind `IsNull`. Ternary, `match` and `?:` merges keep a
        // nullable arm in a `Mixed` temp and never reach here. The default arms of
        // `??`/`??=` are safe the same way: their materialized type is `Object` only because
        // `materialized_expr_type_for_merge` mirrors the result type each lowering actually
        // stamps, so a default can never hand a null-bearing cell to this arm either — keep
        // that mirroring if either side changes.
        PhpType::Object(_) | PhpType::Callable
            if source_ty == PhpType::Mixed && value.ir_type == IrType::Heap(IrHeapKind::Mixed) =>
        {
            let unboxed = ctx.emit_owned_value(
                Op::MixedUnbox,
                vec![value.value],
                None,
                target_ty.clone(),
                Op::mixed_unbox_effects(&target_ty),
                Some(span),
            );
            release_coerced_source_if_owned(ctx, value, Some(span));
            unboxed
        }
        _ => coerce_container_to_mixed_payload(ctx, value, &source_ty, &target_ty, span),
    }
}

/// Widens a typed container value to boxed-Mixed element storage before it is stored.
///
/// Branch merges and stable loop-local contracts can require `Mixed` element storage, so each
/// concrete container must box its slots via `ArrayToMixed` / `HashToMixed`: storing the raw
/// pointer would let Mixed-element reads misinterpret typed slot bytes. Borrowed sources are
/// retained first so the conversion's copy-on-write split rewrites a private copy; owning
/// temporaries transfer their reference into the converted result. A pointer-array target
/// converts boxed indexed sources to pointer words under the same ownership rule.
pub(in crate::ir_lower) fn coerce_container_to_mixed_payload(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    source_ty: &PhpType,
    target_ty: &PhpType,
    span: crate::span::Span,
) -> LoweredValue {
    let target_has_mixed_payload = match target_ty {
        PhpType::Array(elem) => elem.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        _ => false,
    };
    if !target_has_mixed_payload {
        // A TYPED container target with a whole-boxed source is the `?array<int>` shape, and it
        // only exists since `array<T>` did: before that a container merge target was always
        // `array<mixed>`, so this early return could only ever see a source that needed nothing.
        // It stored the CELL into the raw slot untouched, and the first element read then saw the
        // cell header — `function g(?array<int> $a) { $r = $a ?? [4, 5]; echo $r[0]; }` printed
        // `0` and dumped four zeros where php prints `1` and `[1, 2, 3]`. A ternary was correct,
        // and a bare `?array` was correct, which is what kept it hidden.
        //
        // Unboxing is exact here: the payload's slots carry the declared element type, because
        // that declaration is what typed the caller's array in the first place. If that is ever
        // false, the boundary guard in `lower_runtime_call` reads the payload's own value_type
        // tag and fatals rather than handing back a cell pointer as an element.
        if matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
            && value.ir_type == IrType::Heap(IrHeapKind::Mixed)
            && matches!(target_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            let cell_is_owning = ctx.value_is_owning_temporary(value);
            let unboxed = ctx.emit_value(
                Op::RuntimeCall,
                vec![value.value],
                None,
                target_ty.clone(),
                effects_lookup::runtime_effects(),
                Some(span),
            );
            // The unbox takes its own reference to the payload, so an owning cell has no reader
            // left — the same ledger the whole-boxed branch below keeps.
            if cell_is_owning {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
            }
            return unboxed;
        }
        // An indexed array of boxed cells (`function boxed() { $a = [null]; return $a; }` is
        // `array<void>` at the call site) merged into a pointer-array temporary would hand those
        // cells to pointer-word readers, whichever way the merging arm was lowered.
        return super::indexed_array_literals::convert_array_to_pointer_storage(
            ctx, value, target_ty, span,
        );
    }
    let op = match (source_ty, target_ty) {
        (PhpType::Array(source_elem), PhpType::Array(_))
            if source_elem.codegen_repr() != PhpType::Mixed =>
        {
            Op::ArrayToMixed
        }
        (PhpType::AssocArray { value: source_value, .. }, PhpType::AssocArray { .. })
            if source_value.codegen_repr() != PhpType::Mixed =>
        {
            Op::HashToMixed
        }
        (PhpType::Mixed | PhpType::Union(_), _)
            if value.ir_type == IrType::Heap(IrHeapKind::Mixed) =>
        {
            // Whole-boxed sources (a `?array` value flowing through `??`)
            // unbox the cell payload and convert it with the same
            // runtime-call coercion declared container returns use. The
            // conversion borrows the cell and owns a fresh container
            // reference, so an owning cell must be consumed here.
            //
            // The indexed conversion consumes one owned payload reference
            // and rewrites sole-owner arrays in place, which is only sound
            // when the cell owns its payload. A borrowed cell (a `?array`
            // parameter or local) shares its payload with a live caller
            // array, so it unboxes through the owned-payload coercion —
            // which retains the payload — and the consuming `ArrayToMixed`
            // copy-on-write-splits into a private converted copy. The
            // associative helper returns a fresh hash without consuming the
            // payload reference, so borrowed hash cells keep the
            // single-call coercion.
            let cell_is_owning = ctx.value_is_owning_temporary(value);
            if !cell_is_owning && matches!(target_ty, PhpType::Array(_)) {
                let unboxed = ctx.emit_value(
                    Op::RuntimeCall,
                    vec![value.value],
                    None,
                    PhpType::Array(Box::new(PhpType::Never)),
                    effects_lookup::runtime_effects(),
                    Some(span),
                );
                return ctx.emit_value(
                    Op::ArrayToMixed,
                    vec![unboxed.value],
                    None,
                    target_ty.clone(),
                    Op::ArrayToMixed.default_effects(),
                    Some(span),
                );
            }
            let converted = ctx.emit_value(
                Op::RuntimeCall,
                vec![value.value],
                None,
                target_ty.clone(),
                effects_lookup::runtime_effects(),
                Some(span),
            );
            if cell_is_owning {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
            }
            return converted;
        }
        _ => return value,
    };
    // Local loads report as *provisional* owners (their compensating releases
    // are pruned at builder finalization when the slot stays concrete), so
    // they must be treated as borrowed here: without a real retain the
    // conversion's copy-on-write split would never trigger and the local's
    // own array would be boxed in place while its slot type stays concrete.
    let source_is_consumable = ctx.value_is_owning_temporary(value)
        && !ctx.value_is_owned_unboxed_local_load(value.value);
    let source = if source_is_consumable {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
    };
    ctx.emit_value(
        op,
        vec![source.value],
        None,
        target_ty.clone(),
        op.default_effects(),
        Some(span),
    )
}

/// Emits a branch to a target block when the current block can still fall through.
pub(super) fn branch_to(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target, args: Vec::new() });
    }
}

/// Computes definitely initialized slots after a two-way expression split.
pub(super) fn merge_initialized_slots_for_expr(
    split_initialized: &HashSet<LocalSlotId>,
    then_initialized: HashSet<LocalSlotId>,
    then_reachable: bool,
    else_initialized: HashSet<LocalSlotId>,
    else_reachable: bool,
) -> HashSet<LocalSlotId> {
    match (then_reachable, else_reachable) {
        (true, true) => then_initialized
            .intersection(&else_initialized)
            .copied()
            .collect(),
        (true, false) => then_initialized,
        (false, true) => else_initialized,
        (false, false) => split_initialized.clone(),
    }
}

/// Emits a boolean literal value for control-expression lowering.
///
/// Also emits the trailing warn-on-missing flag that boxed-`Mixed` subscript reads
/// pass to `__rt_mixed_array_get`, so every producer of such a read builds the
/// operand the same way.
pub(crate) fn emit_bool_literal(
    ctx: &mut LoweringContext<'_, '_>,
    value: bool,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstBool,
            Vec::new(),
            Some(Immediate::Bool(value)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
            Op::ConstBool.default_effects(),
            span,
        )
        .expect("const_bool produces a value");
    LoweredValue { value, ir_type: IrType::I64 }
}

/// Returns a printable static receiver name.
pub(super) fn receiver_name(receiver: &StaticReceiver) -> String {
    // A receiver written `Box<int>::of()` names `Box` until instantiation renames
    // it, and this pass can run on a generic function's template body — which is
    // walked and then stripped, never instantiated.
    let receiver = &receiver.written_class_receiver();
    match receiver {
        // A generic receiver is instantiated into an ordinary named one before type checking;
        // a template has no class to reach through.
        StaticReceiver::Generic(_) => unreachable!(
            "StaticReceiver::Generic must be instantiated by generics::classes"
        ),
        StaticReceiver::Named(name) => name.as_str().to_string(),
        StaticReceiver::Self_ => "self".to_string(),
        StaticReceiver::Static => "static".to_string(),
        StaticReceiver::Parent => "parent".to_string(),
    }
}

/// Returns a printable callable target name.
pub(super) fn callable_target_name(target: &CallableTarget) -> String {
    match target {
        CallableTarget::Function(name) => name.as_str().to_string(),
        CallableTarget::StaticMethod { receiver, method } => {
            format!("{}::{}", receiver_name(receiver), method)
        }
        CallableTarget::Method { method, .. } => format!("object::{}", method),
    }
}

/// Returns a syntactic fallback PHP type for an expression.
pub(super) fn fallback_expr_type(expr: &Expr) -> PhpType {
    normalize_value_php_type(infer_expr_type_syntactic(expr))
}

/// Normalizes non-materializable expression types to the EIR null sentinel.
pub(super) fn normalize_value_php_type(php_type: PhpType) -> PhpType {
    if matches!(php_type, PhpType::Never) {
        PhpType::Void
    } else {
        php_type
    }
}
