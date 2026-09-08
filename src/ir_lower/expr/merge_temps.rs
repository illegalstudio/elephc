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

/// Chooses a merge temp type from contextual branch materialization and fallback metadata.
pub(super) fn branch_merge_result_type(
    ctx: &LoweringContext<'_, '_>,
    then_expr: &Expr,
    else_expr: &Expr,
    expr: &Expr,
) -> PhpType {
    let then_ty = materialized_expr_type_for_merge(ctx, then_expr);
    let else_ty = materialized_expr_type_for_merge(ctx, else_expr);
    let branch_ty = nullable_aware_branch_merge_type(&then_ty, &else_ty);
    if php_type_allows_null(&branch_ty) {
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
    let mut merged: Option<PhpType> = None;
    for result in arms.iter().map(|(_, result)| result).chain(default) {
        let arm_ty = materialized_expr_type_for_merge(ctx, result);
        merged = Some(match merged {
            Some(acc) => nullable_aware_branch_merge_type(&acc, &arm_ty),
            None => arm_ty,
        });
    }
    let Some(merged) = merged else {
        return fallback_expr_type(expr);
    };
    if php_type_allows_null(&merged) {
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
    let value_ty = materialized_expr_type_for_merge(ctx, value).codegen_repr();
    let default_ty = materialized_expr_type_for_merge(ctx, default).codegen_repr();
    wider_type_for_merge(&value_ty, &default_ty)
}

/// Chooses a ternary branch merge type without erasing PHP null branches.
pub(super) fn nullable_aware_branch_merge_type(left: &PhpType, right: &PhpType) -> PhpType {
    if php_type_allows_null(left) || php_type_allows_null(right) {
        let left_non_null = strip_void_from_union(left.clone());
        let right_non_null = strip_void_from_union(right.clone());
        return normalize_union_members(vec![PhpType::Void, left_non_null, right_non_null])
            .unwrap_or(PhpType::Void);
    }
    wider_type_for_merge(&left.codegen_repr(), &right.codegen_repr())
}

/// Returns true when a PHP type can materialize PHP null at runtime.
pub(super) fn php_type_allows_null(php_type: &PhpType) -> bool {
    match php_type {
        PhpType::Void | PhpType::Never | PhpType::Mixed | PhpType::TaggedScalar => true,
        PhpType::Union(members) => members
            .iter()
            .any(|member| matches!(member, PhpType::Void | PhpType::Never | PhpType::Mixed)),
        _ => false,
    }
}

/// Estimates the value type an expression will materialize during branch lowering.
pub(super) fn materialized_expr_type_for_merge(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> PhpType {
    match &expr.kind {
        ExprKind::Variable(name) => normalize_value_php_type(ctx.local_type(name).codegen_repr()),
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
        ExprKind::ArrayAccess { array, .. } => array_access_expr_value_type_for_ir(ctx, array)
            .unwrap_or_else(|| fallback_expr_type(expr)),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
                .unwrap_or_else(|| fallback_expr_type(expr))
        }
        _ => fallback_expr_type(expr),
    }
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
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never => {
            coerce_to_int_at_span(ctx, value, Some(span))
        }
        PhpType::Float => coerce_to_float_at_span(ctx, value, Some(span)),
        PhpType::Str => coerce_to_string_at_span(ctx, value, Some(span)),
        _ => coerce_container_to_mixed_payload(ctx, value, &source_ty, &target_ty, span),
    }
}

/// Widens a typed container value to boxed-Mixed element storage before it is stored.
///
/// Branch merges and stable loop-local contracts can require `Mixed` element storage, so each
/// concrete container must box its slots via `ArrayToMixed` / `HashToMixed`: storing the raw
/// pointer would let Mixed-element reads misinterpret typed slot bytes. Borrowed sources are
/// retained first so the conversion's copy-on-write split rewrites a private copy; owning
/// temporaries transfer their reference into the converted result.
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
        return value;
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
    match receiver {
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
