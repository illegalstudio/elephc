//! Purpose:
//! Lowers two- and three-level unset chains with explicit COW write-back.
//!
//! Called from:
//! - `crate::ir_lower::expr::unset`.
//!
//! Key details:
//! - Evaluates keys once in PHP source order and never autovivifies missing parents.
//! - Publishes relocated local, instance-property, and static-property containers.

use super::*;
use crate::ir::IrHeapKind;

/// Releases an owning value after a boxed-Mixed nested writer retained or copied it.
///
/// `__rt_mixed_array_set` consumes the fresh Mixed box materialized by codegen, not the original
/// EIR operand: concrete heap values are retained into that box, while an already-Mixed operand is
/// incremented before the call. Borrowed local loads remain owned by their slots and are untouched.
fn release_retained_nested_write_operand(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    let ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if matches!(ty, PhpType::Str) {
        release_persisted_string_operand(ctx, value, span);
    } else if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Walks an `ArrayAccess` chain to its root local and returns outermost-first indices.
///
/// The parser routes single-level writes to `StmtKind::ArrayAssign`, so callers receive at least
/// two indices here. Property/static-property roots return `None` and keep their dedicated paths.
fn nested_local_chain(target: &Expr) -> Option<(&str, Vec<&Expr>)> {
    let mut indices: Vec<&Expr> = Vec::new();
    let mut node = target;
    loop {
        match &node.kind {
            ExprKind::ArrayAccess { array, index } => {
                indices.push(index);
                node = array;
            }
            ExprKind::Variable(name) => {
                // Outermost-first: walking from the assignment leaf collected them in reverse.
                indices.reverse();
                return Some((name, indices));
            }
            _ => return None,
        }
    }
}

/// Returns the statically proven type of one nested-write key.
///
/// The general syntactic key inferer deliberately reports variables as `Mixed`. Nested local
/// write-back can use the lowering's flow-sensitive local fact instead, retaining both integer
/// loop counters and string map keys.
fn nested_index_static_type(ctx: &LoweringContext<'_, '_>, key: &Expr) -> PhpType {
    match &key.kind {
        ExprKind::Variable(name) => ctx.local_type(name).codegen_repr(),
        _ => index_expr_key_type(ctx, key).codegen_repr(),
    }
}

/// Returns whether `arg` is a two- or three-level local unset with concrete write-back support.
///
/// The root can be an indexed or associative local, while each selected child may also cross the
/// boxed-Mixed boundary used by
/// generic PHP arrays. Reference-bound roots use the same path because hash/array mutation
/// lowerers recognize `LoadRefCell` and write any COW-relocated root pointer back into the cell.
/// Deeper chains retain their existing fallback until they have an equally explicit
/// child-to-parent write-back contract.
pub(in crate::ir_lower) fn nested_local_hash_unset_supported(
    ctx: &LoweringContext<'_, '_>,
    arg: &Expr,
) -> bool {
    let Some((name, chain)) = nested_local_chain(arg) else {
        return false;
    };
    if !(2..=3).contains(&chain.len()) {
        return false;
    }
    nested_hash_unset_chain_supported(ctx, ctx.local_type(name), &chain)
}

/// Returns whether every intermediate container and key in a nested unset has concrete lowering.
fn nested_hash_unset_chain_supported(
    ctx: &LoweringContext<'_, '_>,
    root_ty: PhpType,
    chain: &[&Expr],
) -> bool {
    let mut container_ty = root_ty.codegen_repr();
    for (position, key) in chain.iter().enumerate() {
        let key_ty = nested_index_static_type(ctx, key);
        let key_supported = match container_ty.codegen_repr() {
            PhpType::Array(child) => {
                matches!(key_ty, PhpType::Int | PhpType::Bool)
                    || (child.codegen_repr() == PhpType::Mixed
                        || is_empty_indexed_array_element(child.as_ref()))
                        && matches!(key_ty, PhpType::Str | PhpType::Mixed | PhpType::Union(_))
            }
            PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_) => {
                matches!(
                    key_ty,
                    PhpType::Int
                        | PhpType::Bool
                        | PhpType::Str
                        | PhpType::Mixed
                        | PhpType::Union(_)
                )
            }
            _ => false,
        };
        if !key_supported {
            return false;
        }
        if position + 1 == chain.len() {
            return true;
        }
        container_ty = match container_ty.codegen_repr() {
            PhpType::Array(child) if is_empty_indexed_array_element(child.as_ref()) => {
                PhpType::Mixed
            }
            PhpType::Array(child) => child.codegen_repr(),
            PhpType::AssocArray { value, .. } => value.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_) => PhpType::Mixed,
            _ => return false,
        };
    }
    false
}

/// Lowers a supported two- or three-level local `unset` with explicit COW write-back.
///
/// Every key expression is evaluated once in source order. A silent root `isset` probe prevents
/// missing parents from being materialized; the present branch retains the child hash, removes the
/// leaf, writes the possibly relocated hash back into the root container, then releases the
/// temporary share. Returning `false` leaves unsupported shapes to the ordinary unset fallback.
pub(in crate::ir_lower) fn lower_nested_local_hash_unset(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
) -> bool {
    if !nested_local_hash_unset_supported(ctx, target) {
        return false;
    }
    let Some((name, chain)) = nested_local_chain(target) else {
        return false;
    };
    if chain.len() == 3 {
        return lower_three_level_local_hash_unset(ctx, name, &chain, target.span);
    }
    let span = target.span;
    let outer_key_source = lower_expr(ctx, chain[0]);
    let leaf_key = lower_expr(ctx, chain[1]);
    lower_two_level_local_hash_unset_with_values(
        ctx,
        name,
        outer_key_source,
        leaf_key,
        chain[0].span,
        span,
    )
}

/// Lowers a two-level local unset after both key expressions were evaluated in PHP source order.
fn lower_two_level_local_hash_unset_with_values(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    outer_key_source: LoweredValue,
    leaf_key: LoweredValue,
    outer_key_span: Span,
    span: Span,
) -> bool {
    let root = ctx.load_local(name, Some(span));
    let root_ty = ctx.builder.value_php_type(root.value);
    let (outer_key, child_ty, isset_op, get_op, mixed_key_array_read) =
        match root_ty.codegen_repr() {
        PhpType::Array(child)
            if (child.codegen_repr() == PhpType::Mixed
                || is_empty_indexed_array_element(child.as_ref()))
                && matches!(
                    ctx.builder.value_php_type(outer_key_source.value).codegen_repr(),
                    PhpType::Str | PhpType::Mixed | PhpType::Union(_)
                ) =>
        {
            let key = if index_is_boxed_mixed_key(outer_key_source.ir_type) {
                outer_key_source
            } else {
                ctx.box_value_as_mixed(outer_key_source, PhpType::Mixed, Some(outer_key_span))
            };
            (key, PhpType::Mixed, None, None, true)
        }
        PhpType::Array(child) => (
            coerce_to_int_at_span(ctx, outer_key_source, Some(outer_key_span)),
            child.codegen_repr(),
            Some(Op::ArrayIsset),
            Some(Op::ArrayGet),
            false,
        ),
        PhpType::AssocArray { value, .. } => (
            outer_key_source,
            value.codegen_repr(),
            Some(Op::HashIsset),
            Some(Op::HashGet),
            false,
        ),
        PhpType::Mixed | PhpType::Union(_) =>
            (outer_key_source, PhpType::Mixed, None, None, false),
        _ => return false,
    };
    let (probe, probe_true_is_present, prefetched_child) = if mixed_key_array_read {
        let child = ctx.emit_value(
            Op::ArrayGetMixedKeySilent,
            vec![root.value, outer_key.value],
            None,
            PhpType::Mixed,
            Op::ArrayGetMixedKeySilent.default_effects(),
            Some(span),
        );
        let is_null = ctx.emit_value(
            Op::IsNull,
            vec![child.value],
            None,
            PhpType::Bool,
            Op::IsNull.default_effects(),
            Some(span),
        );
        (is_null, false, Some(child))
    } else if let Some(isset_op) = isset_op {
        (
            ctx.emit_value(
                isset_op,
                vec![root.value, outer_key.value],
                None,
                PhpType::Bool,
                isset_op.default_effects(),
                Some(span),
            ),
            true,
            None,
        )
    } else {
        let warning_flag = emit_bool_literal(ctx, false, Some(span));
        let child = ctx.emit_value(
            Op::RuntimeCall,
            vec![root.value, outer_key.value, warning_flag.value],
            None,
            PhpType::Mixed,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        let is_null = ctx.emit_value(
            Op::IsNull,
            vec![child.value],
            None,
            PhpType::Bool,
            Op::IsNull.default_effects(),
            Some(span),
        );
        (is_null, false, Some(child))
    };
    let split_initialized = ctx.initialized_slots_snapshot();
    let present = ctx.builder.create_named_block("unset.nested.present", Vec::new());
    let missing = ctx.builder.create_named_block("unset.nested.missing", Vec::new());
    let done = ctx.builder.create_named_block("unset.nested.done", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: probe.value,
        then_target: if probe_true_is_present { present } else { missing },
        then_args: Vec::new(),
        else_target: if probe_true_is_present { missing } else { present },
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(missing);
    ctx.restore_initialized_slots(split_initialized.clone());
    if let Some(child) = prefetched_child {
        if ctx.value_is_owning_temporary(child) {
            crate::ir_lower::ownership::release_if_owned(ctx, child, Some(span));
        }
    }
    release_nested_unset_operand(ctx, outer_key, span);
    release_nested_unset_operand(ctx, leaf_key, span);
    branch_to(ctx, done);

    ctx.builder.position_at_end(present);
    ctx.restore_initialized_slots(split_initialized);
    let child = if let Some(child) = prefetched_child {
        child
    } else {
        let get_op = get_op.expect("concrete nested unset probes have a matching get op");
        ctx.emit_value(
            get_op,
            vec![root.value, outer_key.value],
            None,
            child_ty.clone(),
            get_op.default_effects(),
            Some(span),
        )
    };
    let rewritten_child = match child_ty.codegen_repr() {
        PhpType::Array(element) => ctx.emit_value(
            Op::ArrayToHash,
            vec![child.value],
            None,
            PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(element.codegen_repr()),
            },
            Op::ArrayToHash.default_effects(),
            Some(span),
        ),
        PhpType::Mixed | PhpType::Union(_) => {
            let hash = ctx.emit_value(
                Op::MixedToHash,
                vec![child.value],
                None,
                PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(PhpType::Mixed),
                },
                Op::MixedToHash.default_effects(),
                Some(span),
            );
            if ctx.value_is_owning_temporary(child) {
                crate::ir_lower::ownership::release_if_owned(ctx, child, Some(span));
            }
            hash
        }
        _ => child,
    };
    ctx.emit_void(
        Op::HashUnset,
        vec![rewritten_child.value, leaf_key.value],
        None,
        Op::HashUnset.default_effects(),
        Some(span),
    );
    release_nested_unset_operand(ctx, leaf_key, span);
    let rewritten_child = if matches!(child_ty, PhpType::Mixed | PhpType::Union(_)) {
        ctx.box_value_as_mixed(rewritten_child, PhpType::Mixed, Some(span))
    } else {
        rewritten_child
    };
    let root_consumes_child = write_nested_local_element_in_place(
        ctx,
        name,
        root,
        root_ty,
        outer_key,
        rewritten_child,
        span,
    );
    if !root_consumes_child && ctx.value_is_owning_temporary(rewritten_child) {
        crate::ir_lower::ownership::release_if_owned(ctx, rewritten_child, Some(span));
    }
    branch_to(ctx, done);

    ctx.builder.position_at_end(done);
    true
}

/// Lowers a three-level local unset by mutating an owned middle container and writing it back.
fn lower_three_level_local_hash_unset(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    chain: &[&Expr],
    span: Span,
) -> bool {
    debug_assert_eq!(chain.len(), 3, "three-level unset requires exactly three keys");
    // PHP evaluates every key expression even when an outer parent is absent. Lower all three
    // before the silent presence probe, then reuse the SSA values in their respective branches.
    let outer_key_source = lower_expr(ctx, chain[0]);
    let middle_key = lower_expr(ctx, chain[1]);
    let leaf_key = lower_expr(ctx, chain[2]);
    let root = ctx.load_local(name, Some(span));
    let root_ty = ctx.builder.value_php_type(root.value);
    let (outer_key, child_ty, isset_op, get_op, mixed_key_array_read) =
        match root_ty.codegen_repr() {
        PhpType::Array(child)
            if (child.codegen_repr() == PhpType::Mixed
                || is_empty_indexed_array_element(child.as_ref()))
                && matches!(
                    ctx.builder.value_php_type(outer_key_source.value).codegen_repr(),
                    PhpType::Str | PhpType::Mixed | PhpType::Union(_)
                ) =>
        {
            let key = if index_is_boxed_mixed_key(outer_key_source.ir_type) {
                outer_key_source
            } else {
                ctx.box_value_as_mixed(outer_key_source, PhpType::Mixed, Some(chain[0].span))
            };
            (key, PhpType::Mixed, None, None, true)
        }
        PhpType::Array(child) => (
            coerce_to_int_at_span(ctx, outer_key_source, Some(chain[0].span)),
            child.codegen_repr(),
            Some(Op::ArrayIsset),
            Some(Op::ArrayGet),
            false,
        ),
        PhpType::AssocArray { value, .. } => (
            outer_key_source,
            value.codegen_repr(),
            Some(Op::HashIsset),
            Some(Op::HashGet),
            false,
        ),
        PhpType::Mixed | PhpType::Union(_) => {
            (outer_key_source, PhpType::Mixed, None, None, false)
        }
        _ => return false,
    };
    let (probe, probe_true_is_present, prefetched_child) = if mixed_key_array_read {
        let child = ctx.emit_value(
            Op::ArrayGetMixedKeySilent,
            vec![root.value, outer_key.value],
            None,
            PhpType::Mixed,
            Op::ArrayGetMixedKeySilent.default_effects(),
            Some(span),
        );
        let is_null = ctx.emit_value(
            Op::IsNull,
            vec![child.value],
            None,
            PhpType::Bool,
            Op::IsNull.default_effects(),
            Some(span),
        );
        (is_null, false, Some(child))
    } else if let Some(isset_op) = isset_op {
        (
            ctx.emit_value(
                isset_op,
                vec![root.value, outer_key.value],
                None,
                PhpType::Bool,
                isset_op.default_effects(),
                Some(span),
            ),
            true,
            None,
        )
    } else {
        let warning_flag = emit_bool_literal(ctx, false, Some(span));
        let child = ctx.emit_value(
            Op::RuntimeCall,
            vec![root.value, outer_key.value, warning_flag.value],
            None,
            PhpType::Mixed,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        let is_null = ctx.emit_value(
            Op::IsNull,
            vec![child.value],
            None,
            PhpType::Bool,
            Op::IsNull.default_effects(),
            Some(span),
        );
        (is_null, false, Some(child))
    };
    let split_initialized = ctx.initialized_slots_snapshot();
    let present = ctx.builder.create_named_block("unset.deep.present", Vec::new());
    let missing = ctx.builder.create_named_block("unset.deep.missing", Vec::new());
    let done = ctx.builder.create_named_block("unset.deep.done", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: probe.value,
        then_target: if probe_true_is_present { present } else { missing },
        then_args: Vec::new(),
        else_target: if probe_true_is_present { missing } else { present },
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(missing);
    ctx.restore_initialized_slots(split_initialized.clone());
    if let Some(child) = prefetched_child {
        if ctx.value_is_owning_temporary(child) {
            crate::ir_lower::ownership::release_if_owned(ctx, child, Some(span));
        }
    }
    release_nested_unset_operand(ctx, outer_key, span);
    release_nested_unset_operand(ctx, middle_key, span);
    release_nested_unset_operand(ctx, leaf_key, span);
    branch_to(ctx, done);

    ctx.builder.position_at_end(present);
    ctx.restore_initialized_slots(split_initialized);
    let child = if let Some(child) = prefetched_child {
        child
    } else {
        let get_op = get_op.expect("concrete deep unset probes have a matching get op");
        ctx.emit_value(
            get_op,
            vec![root.value, outer_key.value],
            None,
            child_ty.clone(),
            get_op.default_effects(),
            Some(span),
        )
    };
    // A boxed Mixed child belongs to the root bucket. Retaining that cell into the hidden temp
    // would keep the same mutable wrapper shared with every COW alias of the root (for example a
    // value returned from an array property before this unset). Clone the child container now,
    // then mutate and write that independent cell back through the outer root. The next nested
    // level performs its own hash clone before removing the leaf, completing PHP's path-copying
    // discipline without eagerly deep-cloning unrelated descendants.
    let child = if matches!(child_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        let separated_hash = ctx.emit_value(
            Op::MixedToHash,
            vec![child.value],
            None,
            PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            },
            Op::MixedToHash.default_effects(),
            Some(span),
        );
        if ctx.value_is_owning_temporary(child) {
            crate::ir_lower::ownership::release_if_owned(ctx, child, Some(span));
        }
        ctx.box_value_as_mixed(separated_hash, PhpType::Mixed, Some(span))
    } else {
        child
    };
    let temp_name = ctx.declare_owned_hidden_temp(child_ty.clone());
    store_value_into_temp(ctx, &temp_name, child_ty.clone(), child, span);
    if !lower_two_level_local_hash_unset_with_values(
        ctx,
        &temp_name,
        middle_key,
        leaf_key,
        chain[1].span,
        span,
    ) {
        return false;
    }
    let rewritten_child = ctx.load_local(&temp_name, Some(span));
    let root_consumes_child = write_nested_local_element_in_place(
        ctx,
        name,
        root,
        root_ty,
        outer_key,
        rewritten_child,
        span,
    );
    release_owned_hidden_temp(
        ctx,
        &temp_name,
        child_ty,
        root_consumes_child,
        span,
    );
    branch_to(ctx, done);

    ctx.builder.position_at_end(done);
    true
}

/// Walks an array-access chain rooted at a named instance property.
///
/// Returned indices are outermost-first, matching PHP evaluation order and the local nested-unset
/// helper's expected shape.
fn nested_property_chain(target: &Expr) -> Option<(&Expr, &str, Vec<&Expr>)> {
    let mut indices = Vec::new();
    let mut node = target;
    loop {
        match &node.kind {
            ExprKind::ArrayAccess { array, index } => {
                indices.push(index.as_ref());
                node = array;
            }
            ExprKind::PropertyAccess { object, property } => {
                indices.reverse();
                return Some((object, property, indices));
            }
            _ => return None,
        }
    }
}

/// Walks an array-access chain rooted at a named static property.
fn nested_static_property_chain(
    target: &Expr,
) -> Option<(&StaticReceiver, &str, Vec<&Expr>)> {
    let mut indices = Vec::new();
    let mut node = target;
    loop {
        match &node.kind {
            ExprKind::ArrayAccess { array, index } => {
                indices.push(index.as_ref());
                node = array;
            }
            ExprKind::StaticPropertyAccess { receiver, property } => {
                indices.reverse();
                return Some((receiver, property, indices));
            }
            _ => return None,
        }
    }
}

/// Returns whether a two- or three-level static-property unset can reuse local COW write-back.
pub(in crate::ir_lower) fn nested_static_property_hash_unset_supported(
    ctx: &LoweringContext<'_, '_>,
    target: &Expr,
) -> bool {
    let Some((receiver, property, chain)) = nested_static_property_chain(target) else {
        return false;
    };
    if !(2..=3).contains(&chain.len()) {
        return false;
    }
    let Some(property_ty) = static_property_type(ctx, receiver, property) else {
        return false;
    };
    nested_hash_unset_chain_supported(ctx, property_ty, &chain)
}

/// Lowers a two- or three-level static-property unset through an owned hidden local.
pub(in crate::ir_lower) fn lower_nested_static_property_hash_unset(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
) -> bool {
    if !nested_static_property_hash_unset_supported(ctx, target) {
        return false;
    }
    let Some((receiver, property, chain)) = nested_static_property_chain(target) else {
        return false;
    };
    let Some(property_ty) = static_property_type(ctx, receiver, property) else {
        return false;
    };
    let span = target.span;
    let property_value = load_static_property_as(
        ctx,
        receiver,
        property,
        property_ty.clone(),
        span,
    );
    let temp_name = ctx.declare_owned_hidden_temp(property_ty.clone());
    store_value_into_temp(ctx, &temp_name, property_ty.clone(), property_value, span);
    let mut synthetic = Expr::new(ExprKind::Variable(temp_name.clone()), span);
    for index in chain {
        synthetic = Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(synthetic),
                index: Box::new(index.clone()),
            },
            span,
        );
    }
    if !lower_nested_local_hash_unset(ctx, &synthetic) {
        return false;
    }
    let rewritten = ctx.load_local(&temp_name, Some(span));
    ctx.clear_owned_hidden_temp(&temp_name, Some(span));
    store_static_property(ctx, receiver, property, rewritten.value, span);
    if static_property_store_retains_independent_value(
        ctx,
        receiver,
        property,
        rewritten,
    ) && ctx.value_is_owning_temporary(rewritten)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, rewritten, Some(span));
    }
    true
}

/// Returns whether a two- or three-level instance-property unset can reuse local COW write-back.
pub(in crate::ir_lower) fn nested_property_hash_unset_supported(
    ctx: &LoweringContext<'_, '_>,
    target: &Expr,
) -> bool {
    let Some((object, property, chain)) = nested_property_chain(target) else {
        return false;
    };
    if !(2..=3).contains(&chain.len()) {
        return false;
    }
    let Some(property_ty) = property_access_expr_type_for_ir(ctx, object, property) else {
        return false;
    };
    nested_hash_unset_chain_supported(ctx, property_ty, &chain)
}

/// Lowers a two- or three-level instance-property unset through an owned hidden local.
///
/// Copying the property into the temp deliberately creates the COW share that the local helper
/// separates before deleting the leaf. The possibly relocated root is then moved back through the
/// retaining property store, so both present and missing-parent paths keep balanced ownership.
pub(in crate::ir_lower) fn lower_nested_property_hash_unset(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
) -> bool {
    if !nested_property_hash_unset_supported(ctx, target) {
        return false;
    }
    let Some((object_expr, property, chain)) = nested_property_chain(target) else {
        return false;
    };
    let Some(property_ty) = property_access_expr_type_for_ir(ctx, object_expr, property) else {
        return false;
    };
    let span = target.span;
    let object = lower_expr(ctx, object_expr);
    let data = ctx.intern_string(property);
    let property_value = ctx.emit_value(
        Op::PropGet,
        vec![object.value],
        Some(Immediate::Data(data)),
        property_ty.clone(),
        Op::PropGet.default_effects(),
        Some(span),
    );
    let temp_name = ctx.declare_owned_hidden_temp(property_ty.clone());
    store_value_into_temp(ctx, &temp_name, property_ty.clone(), property_value, span);
    let mut synthetic = Expr::new(ExprKind::Variable(temp_name.clone()), span);
    for index in chain {
        synthetic = Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(synthetic),
                index: Box::new(index.clone()),
            },
            span,
        );
    }
    if !lower_nested_local_hash_unset(ctx, &synthetic) {
        return false;
    }
    let rewritten = ctx.load_local(&temp_name, Some(span));
    ctx.clear_owned_hidden_temp(&temp_name, Some(span));
    ctx.emit_void(
        Op::PropSet,
        vec![object.value, rewritten.value],
        Some(Immediate::Data(data)),
        Op::PropSet.default_effects(),
        Some(span),
    );
    release_rewritten_property_value_after_retaining_store(
        ctx,
        &property_ty,
        rewritten,
        span,
    );
    if ctx.value_is_owning_temporary(object) {
        crate::ir_lower::ownership::release_if_owned(ctx, object, Some(span));
    }
    true
}

/// Releases a nested-unset key temporary after the runtime consumer has borrowed it.
fn release_nested_unset_operand(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    } else {
        release_persisted_string_operand(ctx, value, span);
    }
}

/// Writes a nested child back into a named local and reports whether the child was consumed.
///
/// Generic `array<mixed>` locals can already carry promoted hash storage at runtime. Their boxed
/// Mixed/string keys therefore use the heap-kind-aware mixed-key setter and explicitly store its
/// possibly relocated pointer back into the local. Concrete keys and container shapes retain the
/// ordinary in-place write path.
fn write_nested_local_element_in_place(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    container: LoweredValue,
    container_ty: PhpType,
    key: LoweredValue,
    value: LoweredValue,
    span: Span,
) -> bool {
    let generic_mixed_array = matches!(
        container_ty.codegen_repr(),
        PhpType::Array(element)
            if element.codegen_repr() == PhpType::Mixed
                || is_empty_indexed_array_element(element.as_ref())
    );
    if container.ir_type == IrType::Heap(IrHeapKind::Array)
        && generic_mixed_array
        && index_is_boxed_mixed_key(key.ir_type)
    {
        let stored_ty = PhpType::Array(Box::new(PhpType::Mixed));
        let result = ctx.emit_value(
            Op::ArraySetMixedKey,
            vec![container.value, key.value, value.value],
            None,
            stored_ty.clone(),
            Op::ArraySetMixedKey.default_effects(),
            Some(span),
        );
        ctx.store_mutated_local(name, result, stored_ty, Some(span));
        release_nested_unset_operand(ctx, key, span);
        return false;
    }

    // A Mixed-element array consumes only a value it boxes itself; an already-boxed child is
    // retained by the set (`lower_mixed_array_set_*`), so its temp keeps its own share. Treating
    // that share as consumed leaked the rewritten element of every `unset($trace[$i]['args'])`
    // over a `debug_backtrace()` result: the frame hash, its strings and its argument array.
    let consumes_child = nested_parent_consumes_child(container.ir_type, &container_ty)
        && !(container.ir_type == IrType::Heap(IrHeapKind::Array)
            && matches!(
                ctx.builder.value_php_type(value.value).codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            ));
    write_nested_element_in_place(ctx, container, container_ty, key, value, span);
    consumes_child
}

/// Returns whether a nested parent write transfers the owned child out of its temp slot.
///
/// Concrete refcounted indexed arrays call `__rt_array_set_refcounted`, which retains the
/// incoming payload; the temp must therefore release its original share after write-back.
/// Mixed-element arrays and boxed-Mixed receivers instead consume the owned child while boxing
/// or installing it. Hash setters retain their incoming refcounted payload like concrete arrays.
fn nested_parent_consumes_child(parent_ir: IrType, parent_ty: &PhpType) -> bool {
    match parent_ir {
        IrType::Heap(IrHeapKind::Hash) => false,
        IrType::Heap(IrHeapKind::Array) => matches!(
            parent_ty.codegen_repr(),
            PhpType::Array(element_ty) if element_ty.codegen_repr() == PhpType::Mixed
        ),
        IrType::Heap(IrHeapKind::Mixed) | IrType::Heap(IrHeapKind::Union) => true,
        _ => true,
    }
}

/// Writes `value` into `container` at `key` in place via the matching 3-operand set op:
/// `Heap(Hash)` → `HashSet` (or `HashAppend` for a `[]` append — currently unused since the parser
/// lowers `$x[$k][]` to a read+push+writeback sequence); `Heap(Array)` → `ArraySet` (integer key
/// coerced); `Heap(Mixed)`/`Heap(Union)` → `__rt_mixed_array_set` (3-operand `RuntimeCall`). The
/// set op's codegen releases the displaced prior element and updates the container SSA value's
/// home with the possibly-relocated pointer.
fn write_nested_element_in_place(
    ctx: &mut LoweringContext<'_, '_>,
    container: LoweredValue,
    container_ty: PhpType,
    key: LoweredValue,
    value: LoweredValue,
    span: Span,
) {
    match container.ir_type {
        IrType::Heap(IrHeapKind::Hash) => {
            ctx.emit_void(
                Op::HashSet,
                vec![container.value, key.value, value.value],
                None,
                Op::HashSet.default_effects(),
                Some(span),
            );
            release_nested_unset_operand(ctx, key, span);
            release_persisted_string_operand(ctx, value, span);
        }
        IrType::Heap(IrHeapKind::Array) => {
            let key_int = coerce_to_int_at_span(ctx, key, Some(span));
            let value_coerced = coerce_indexed_array_set_value(ctx, &container_ty, value, Some(span));
            ctx.emit_void(
                Op::ArraySet,
                vec![container.value, key_int.value, value_coerced.value],
                None,
                Op::ArraySet.default_effects(),
                Some(span),
            );
            release_persisted_string_operand(ctx, value_coerced, span);
        }
        IrType::Heap(IrHeapKind::Mixed) | IrType::Heap(IrHeapKind::Union) => {
            ctx.emit_void(
                Op::RuntimeCall,
                vec![container.value, key.value, value.value],
                None,
                effects_lookup::runtime_effects(),
                Some(span),
            );
            release_nested_unset_operand(ctx, key, span);
            release_retained_nested_write_operand(ctx, value, span);
        }
        _ => {
            // Defensive: the checker gates scalar-as-array for reference-bound bases. Fall back to
            // the 2-operand runtime cell assign, which loud-errors at codegen for a scalar receiver.
            ctx.emit_void(
                Op::RuntimeCall,
                vec![container.value, value.value],
                None,
                effects_lookup::runtime_effects(),
                Some(span),
            );
            release_persisted_string_operand(ctx, value, span);
        }
    }
}

/// Releases an owned hidden temp's redundant share with the temp's actual type and clears the
/// backing slot without an additional release (SLICE-2 `:787-803` discipline). After a `HashSet`
/// write-back, the parent slot retains the value (HashSet incref's before storing), so the temp's
/// share is the redundant one and must be decref'd through the matching runtime helper (not via
/// `unset`, which would widen the slot toward `Void`/`Mixed` and decref through the wrong helper,
/// leaking the heap object).
///
/// Boxed-Mixed `ArraySet` and 3-operand `__rt_mixed_array_set` consume the owned child while
/// boxing/installing it, so the temp no longer holds a live reference and is only cleared.
/// Concrete refcounted `ArraySet` instead retains the child and therefore uses `consumed = false`.
fn release_owned_hidden_temp(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    ty: PhpType,
    consumed: bool,
    span: Span,
) {
    if !consumed {
        let slot = ctx.declare_local(name, ty);
        ctx.release_stored_local_value(name, slot, Some(span));
    }
    ctx.clear_owned_hidden_temp(name, Some(span));
}
