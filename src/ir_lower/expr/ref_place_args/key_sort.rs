//! Purpose:
//! Lowers `ksort()` and `krsort()` when their by-reference receiver is a property or
//! nested array cell that needs explicit promotion, copy-on-write, or write-back.
//!
//! Called from:
//! - `super::lower_builtin_ref_place_call()` after shared call-argument validation.
//!
//! Key details:
//! - Descending order promotes packed receivers to hashes so numeric keys can be relinked.
//! - Nested Mixed cells are cloned before promotion when a shallow parent alias could observe
//!   the mutation; attached write-fetched cells retain their parent-owned storage contract.
//! - Non-local heterogeneous parents are stabilized into a retained temporary, sorted through
//!   the local path, and written back to their property or containing element.

use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::ir::{ArrayKeySort, Immediate, Op};
use crate::names::{php_symbol_key, property_hook_get_method, property_hook_set_method};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

use super::super::lower_expr;
use super::{place_object_class_name, ref_param_place, static_place_type};

/// Attempts the specialized property or nested-cell lowering for a PHP key sort.
pub(super) fn lower_key_sort_ref_place_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: &FunctionSig,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let sort = match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "ksort" => ArrayKeySort::Ascending,
        "krsort" => ArrayKeySort::Descending,
        _ => return None,
    };
    if let Some(result) = lower_direct_property_key_sort(ctx, name, sig, args, expr, sort) {
        return Some(result);
    }
    if let Some(result) = lower_exact_php_array_place_key_sort(ctx, name, sig, args, expr, sort) {
        return Some(result);
    }
    lower_mixed_array_element_key_sort(ctx, name, sig, args, expr, sort)
}

/// Sorts a direct property through its concrete container or declared-array boxed cell.
fn lower_direct_property_key_sort(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: &FunctionSig,
    args: &[Expr],
    expr: &Expr,
    sort: ArrayKeySort,
) -> Option<LoweredValue> {
    let [arg] = args else {
        return None;
    };
    let place = ref_param_place(sig, 0, arg)?;
    let ExprKind::PropertyAccess { object, property } = &place.kind else {
        return None;
    };
    if !matches!(object.kind, ExprKind::Variable(_) | ExprKind::This)
        || property_requires_generic_write_context(ctx, object, property)
    {
        return None;
    }
    let property_ty = static_place_type(ctx, place)?;
    if property_ty.is_php_array() {
        let cell = super::super::lower_by_ref_foreach_property_source(
            ctx, object, property, place,
        );
        return Some(sort_attached_mixed_cell(ctx, name, cell, expr, sort));
    }
    if sort != ArrayKeySort::Descending {
        return None;
    }
    let property_ty = property_ty.codegen_repr();
    if !matches!(&property_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return None;
    }

    let object = lower_expr(ctx, object);
    let property_value = super::super::property_access::lower_property_get_from_value(
        ctx,
        object,
        property,
        Op::PropGet,
        place,
    );
    let property_value =
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, property_value, Some(place.span));
    let hash = match property_ty {
        PhpType::Array(element_ty) => {
            let assoc_ty = PhpType::AssocArray {
                key: Box::new(PhpType::Int),
                value: element_ty,
            };
            ctx.emit_value(
                Op::ArrayToHash,
                vec![property_value.value],
                None,
                assoc_ty,
                Op::ArrayToHash.default_effects(),
                Some(place.span),
            )
        }
        PhpType::AssocArray { .. } => property_value,
        _ => return None,
    };
    Some(super::super::emit_builtin_call_value(
        ctx,
        name,
        vec![hash.value],
        PhpType::Bool,
        expr.span,
        None,
    ))
}

/// Reports whether hooks or readonly enforcement require the generic property write path.
fn property_requires_generic_write_context(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> bool {
    let Some(class_name) = place_object_class_name(ctx, object) else {
        return true;
    };
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return true;
    };
    let getter = php_symbol_key(&property_hook_get_method(property));
    let setter = php_symbol_key(&property_hook_set_method(property));
    class_info.readonly_properties.contains(property)
        || class_info.methods.contains_key(&getter)
        || class_info.methods.contains_key(&setter)
}

/// Sorts an exact PHP `array` local or general writable place through its boxed Mixed cell.
///
/// Plain locals own an independent PHP value, including exact `array` parameters cloned into a
/// dominating copy-on-write shadow at function entry. Ref-bound locals intentionally keep their
/// shared cell. General places first clone one stable read into an ordinary PHP local, then write
/// the sorted cell back through the existing property, static-property, hook, readonly, and
/// nested-place machinery.
fn lower_exact_php_array_place_key_sort(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: &FunctionSig,
    args: &[Expr],
    expr: &Expr,
    sort: ArrayKeySort,
) -> Option<LoweredValue> {
    let [arg] = args else {
        return None;
    };
    let place = ref_param_place(sig, 0, arg)?;
    if !static_place_type(ctx, place)?.is_php_array() {
        return None;
    }
    if let ExprKind::Variable(local) = &place.kind {
        let cell = ctx.load_local(local, Some(place.span));
        return Some(sort_attached_mixed_cell(ctx, name, cell, expr, sort));
    }
    if !super::is_candidate_place_shape(place) {
        return None;
    }

    let place = super::stabilize_place(ctx, place);
    let source = lower_expr(ctx, &place);
    let work_cell = clone_mixed_cell(ctx, source, expr);
    let temp = ctx.declare_synthetic_php_local(PhpType::Mixed);
    ctx.store_local(&temp, work_cell, PhpType::Mixed, Some(place.span));

    let cell = ctx.load_local(&temp, Some(place.span));
    let result = sort_attached_mixed_cell(ctx, name, cell, expr, sort);
    let temp_value = Expr::new(ExprKind::Variable(temp.clone()), place.span);
    super::lower_non_local_assignment_write(ctx, &place, &temp_value, place.span);

    let slot = ctx.declare_local(&temp, PhpType::Mixed);
    ctx.emit_void(
        Op::ReleaseLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(),
        Some(expr.span),
    );
    Some(result)
}

/// Sorts one nested array cell through an independently mutable boxed-Mixed payload.
///
/// Direct packed or associative parents are first COW-separated and widened. Parents that already
/// store Mixed cells instead clone and republish only the selected cell, preventing shallow parent
/// aliases from observing promotion. The guarded runtime accepts tag 4 (promote), tag 5 (borrow),
/// and raises `TypeError` for every other tag.
fn lower_mixed_array_element_key_sort(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: &FunctionSig,
    args: &[Expr],
    expr: &Expr,
    sort: ArrayKeySort,
) -> Option<LoweredValue> {
    let [arg] = args else {
        return None;
    };
    let place = ref_param_place(sig, 0, arg)?;
    let ExprKind::ArrayAccess { array, index } = &place.kind else {
        return None;
    };
    if static_place_type(ctx, array).is_some_and(|ty| ty.is_php_array()) {
        return Some(lower_boxed_parent_element_key_sort(
            ctx, name, place, expr, sort,
        ));
    }
    let ExprKind::Variable(parent_name) = &array.kind else {
        return lower_non_local_mixed_array_element_key_sort(
            ctx, name, sig, arg, place, array, index, expr, sort,
        );
    };
    if let PhpType::Array(element_ty) = ctx.local_type(parent_name).codegen_repr() {
        if sort == ArrayKeySort::Ascending && element_ty.codegen_repr() != PhpType::Mixed {
            return None;
        }
        return lower_mixed_packed_array_element_key_sort(
            ctx,
            name,
            parent_name,
            array,
            index,
            expr,
            *element_ty,
            sort,
        );
    }
    let PhpType::AssocArray { key, value } = ctx.local_type(parent_name).codegen_repr() else {
        return None;
    };
    let value_repr = value.codegen_repr();
    if sort == ArrayKeySort::Ascending && value_repr != PhpType::Mixed {
        return None;
    }
    if !matches!(
        &value_repr,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    ) {
        return None;
    }

    let mixed_parent_ty = PhpType::AssocArray {
        key,
        value: Box::new(PhpType::Mixed),
    };
    if value_repr != PhpType::Mixed {
        let parent = ctx.load_local(parent_name, Some(array.span));
        ctx.prepare_mutated_local_owner(
            parent_name,
            parent,
            mixed_parent_ty.clone(),
            Some(array.span),
        );
        let mixed_parent = ctx.emit_value(
            Op::HashToMixed,
            vec![parent.value],
            None,
            mixed_parent_ty.clone(),
            Op::HashToMixed.default_effects(),
            Some(array.span),
        );
        ctx.store_prepared_mutated_local(
            parent_name,
            mixed_parent,
            mixed_parent_ty,
            Some(array.span),
        );
    }

    let parent = ctx.load_local(parent_name, Some(array.span));
    let key = lower_expr(ctx, index);
    let cell_op = if value_repr == PhpType::Mixed {
        Op::HashGet
    } else {
        Op::HashGetForWrite
    };
    let cell = ctx.emit_value(
        cell_op,
        vec![parent.value, key.value],
        None,
        PhpType::Mixed,
        cell_op.default_effects(),
        Some(expr.span),
    );
    if value_repr == PhpType::Mixed {
        return lower_shared_mixed_hash_element_key_sort(
            ctx, name, parent, key, cell, expr, sort,
        );
    }
    lower_attached_mixed_cell_key_sort(ctx, name, cell, expr, sort)
}

/// Sorts one child of an exact PHP `array` place through an owned Mixed work cell.
///
/// The place and any side-effecting index are stabilized before the read. The temporary owns an
/// independent PHP value while runtime promotion validates and sorts its boxed payload. A normal
/// return writes the cell back through existing place COW and reference handling, then retires the
/// temporary; an exceptional return leaves frame cleanup responsible for that same owner.
fn lower_boxed_parent_element_key_sort(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    place: &Expr,
    expr: &Expr,
    sort: ArrayKeySort,
) -> LoweredValue {
    let place = super::stabilize_place(ctx, place);
    let child = lower_expr(ctx, &place);
    let child = clone_mixed_cell(ctx, child, expr);
    let temp = ctx.declare_synthetic_php_local(PhpType::Mixed);
    ctx.store_local(&temp, child, PhpType::Mixed, Some(place.span));

    let cell = ctx.load_local(&temp, Some(place.span));
    let result = sort_attached_mixed_cell(ctx, name, cell, expr, sort);
    let temp_value = Expr::new(ExprKind::Variable(temp.clone()), place.span);
    super::lower_non_local_assignment_write(ctx, &place, &temp_value, place.span);

    let slot = ctx.declare_local(&temp, PhpType::Mixed);
    ctx.emit_void(
        Op::ReleaseLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(),
        Some(expr.span),
    );
    result
}

/// Sorts a Mixed child of a property or nested parent through a retained local parent copy.
///
/// The parent is stabilized before it is read, then written back after the existing local-parent
/// lowering has performed COW separation, guarded child promotion, and key sorting. This preserves
/// PHP mutation semantics for `$this->grid[0]`, `$object->rows[$key]`, and deeper supported places.
#[allow(clippy::too_many_arguments)]
fn lower_non_local_mixed_array_element_key_sort(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: &FunctionSig,
    original_arg: &Expr,
    place: &Expr,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
    sort: ArrayKeySort,
) -> Option<LoweredValue> {
    let parent_ty = super::static_place_type(ctx, array)?.codegen_repr();
    let supported = match &parent_ty {
        PhpType::Array(element_ty) => {
            element_ty.codegen_repr() == PhpType::Mixed
                && super::super::index_expr_key_type(ctx, index) == PhpType::Int
        }
        PhpType::AssocArray { value, .. } => {
            value.codegen_repr() == PhpType::Mixed
                && matches!(
                    super::super::index_expr_key_type(ctx, index),
                    PhpType::Int | PhpType::Str | PhpType::Mixed
                )
        }
        _ => false,
    };
    if !supported {
        return None;
    }

    let stabilized = super::stabilize_place(ctx, place);
    let ExprKind::ArrayAccess {
        array: stabilized_parent,
        index: stabilized_index,
    } = &stabilized.kind
    else {
        return None;
    };
    let parent_value = lower_expr(ctx, stabilized_parent);
    let parent_temp = ctx.declare_synthetic_php_local(parent_ty.clone());
    ctx.store_local(
        &parent_temp,
        parent_value,
        parent_ty,
        Some(stabilized_parent.span),
    );
    let temp_parent = Expr::new(
        ExprKind::Variable(parent_temp.clone()),
        stabilized_parent.span,
    );
    let nested_place = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(temp_parent.clone()),
            index: stabilized_index.clone(),
        },
        place.span,
    );
    let nested_arg = match &original_arg.kind {
        ExprKind::NamedArg { name, .. } => Expr::new(
            ExprKind::NamedArg {
                name: name.clone(),
                value: Box::new(nested_place),
            },
            original_arg.span,
        ),
        _ => nested_place,
    };
    let result = lower_mixed_array_element_key_sort(
        ctx,
        name,
        sig,
        std::slice::from_ref(&nested_arg),
        expr,
        sort,
    )?;
    super::lower_non_local_assignment_write(
        ctx,
        stabilized_parent,
        &temp_parent,
        stabilized_parent.span,
    );
    Some(result)
}

/// Sorts one packed-parent Mixed cell, widening the parent only when it is still concrete.
///
/// The first nested sort converts `array<array<T>>` to stored Mixed cells after separating the
/// parent for copy-on-write. Later sibling sorts reuse the resulting `array<mixed>` directly so
/// `ArrayToMixed` never receives an already-Mixed input and the guarded cell promotion remains the
/// sole authority for accepting a packed child, borrowing a promoted hash, or raising `TypeError`.
fn lower_mixed_packed_array_element_key_sort(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    parent_name: &str,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
    element_ty: PhpType,
    sort: ArrayKeySort,
) -> Option<LoweredValue> {
    let element_repr = element_ty.codegen_repr();
    if !matches!(&element_repr, PhpType::Array(_) | PhpType::Mixed)
        || super::super::index_expr_key_type(ctx, index) != PhpType::Int
    {
        return None;
    }
    let mixed_parent_ty = PhpType::Array(Box::new(PhpType::Mixed));
    if element_repr != PhpType::Mixed {
        let parent = ctx.load_local(parent_name, Some(array.span));
        ctx.prepare_mutated_local_owner(
            parent_name,
            parent,
            mixed_parent_ty.clone(),
            Some(array.span),
        );
        let mixed_parent = ctx.emit_value(
            Op::ArrayToMixed,
            vec![parent.value],
            None,
            mixed_parent_ty.clone(),
            Op::ArrayToMixed.default_effects(),
            Some(array.span),
        );
        ctx.store_prepared_mutated_local(
            parent_name,
            mixed_parent,
            mixed_parent_ty,
            Some(array.span),
        );
    }

    let parent = ctx.load_local(parent_name, Some(array.span));
    let key = lower_expr(ctx, index);
    let key = super::super::coerce_to_int_at_span(ctx, key, Some(index.span));
    let cell_op = if element_repr == PhpType::Mixed {
        Op::ArrayGet
    } else {
        Op::ArrayGetForWrite
    };
    let cell = ctx.emit_value(
        cell_op,
        vec![parent.value, key.value],
        None,
        PhpType::Mixed,
        cell_op.default_effects(),
        Some(expr.span),
    );
    if element_repr == PhpType::Mixed {
        return lower_shared_mixed_array_element_key_sort(
            ctx, name, parent, key, cell, expr, sort,
        );
    }
    lower_attached_mixed_cell_key_sort(ctx, name, cell, expr, sort)
}

/// Detaches one shared associative-parent cell before publishing and sorting its promoted hash.
///
/// Parent COW is performed by `HashSet`; cloning first prevents a shallow parent split from
/// exposing an in-place cell promotion through aliases. Failed promotion occurs before insertion,
/// so missing or scalar elements keep the guarded `TypeError` path without autovivification.
fn lower_shared_mixed_hash_element_key_sort(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    parent: LoweredValue,
    key: LoweredValue,
    cell: LoweredValue,
    expr: &Expr,
    sort: ArrayKeySort,
) -> Option<LoweredValue> {
    let cloned = clone_mixed_cell(ctx, cell, expr);
    let hash = promote_mixed_cell_to_hash(ctx, cloned, expr, sort);
    ctx.emit_void(
        Op::HashSet,
        vec![parent.value, key.value, cloned.value],
        None,
        Op::HashSet.default_effects(),
        Some(expr.span),
    );
    let result = super::super::emit_builtin_call_value(
        ctx,
        name,
        vec![hash.value],
        PhpType::Bool,
        expr.span,
        None,
    );
    crate::ir_lower::ownership::release_if_owned(ctx, cloned, Some(expr.span));
    Some(result)
}

/// Detaches one shared packed-parent cell before publishing and sorting its promoted hash.
///
/// `ArraySet` performs the parent COW split only after guarded promotion succeeds, preserving the
/// absent/scalar failure behavior while installing an independently owned cell for mutation.
fn lower_shared_mixed_array_element_key_sort(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    parent: LoweredValue,
    key: LoweredValue,
    cell: LoweredValue,
    expr: &Expr,
    sort: ArrayKeySort,
) -> Option<LoweredValue> {
    let cloned = clone_mixed_cell(ctx, cell, expr);
    let hash = promote_mixed_cell_to_hash(ctx, cloned, expr, sort);
    ctx.emit_void(
        Op::ArraySet,
        vec![parent.value, key.value, cloned.value],
        None,
        Op::ArraySet.default_effects(),
        Some(expr.span),
    );
    let result = super::super::emit_builtin_call_value(
        ctx,
        name,
        vec![hash.value],
        PhpType::Bool,
        expr.span,
        None,
    );
    crate::ir_lower::ownership::release_if_owned(ctx, cloned, Some(expr.span));
    Some(result)
}

/// Clones a stored Mixed cell and releases the borrowed/owned source handle.
fn clone_mixed_cell(
    ctx: &mut LoweringContext<'_, '_>,
    cell: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let cloned = ctx.emit_owned_value(
        Op::RuntimeCall,
        vec![cell.value],
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::MixedCellClone,
        )),
        PhpType::Mixed,
        super::super::effects_lookup::runtime_effects(),
        Some(expr.span),
    );
    crate::ir_lower::ownership::release_if_owned(ctx, cell, Some(expr.span));
    cloned
}

/// Promotes a guarded Mixed cell to the borrowed hash representation consumed by a key sort.
fn promote_mixed_cell_to_hash(
    ctx: &mut LoweringContext<'_, '_>,
    cell: LoweredValue,
    expr: &Expr,
    sort: ArrayKeySort,
) -> LoweredValue {
    ctx.emit_value(
        Op::RuntimeCall,
        vec![cell.value],
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::MixedCellPromoteToHash(sort),
        )),
        PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Mixed),
        },
        super::super::effects_lookup::runtime_effects(),
        Some(expr.span),
    )
}

/// Promotes a write-fetched cell and marks the returned hash as attached to its parent place.
fn promote_attached_mixed_cell_to_hash(
    ctx: &mut LoweringContext<'_, '_>,
    cell: LoweredValue,
    expr: &Expr,
    sort: ArrayKeySort,
) -> LoweredValue {
    ctx.emit_value(
        Op::RuntimeCall,
        vec![cell.value],
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::MixedCellPromoteAttachedToHash(sort),
        )),
        PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Mixed),
        },
        super::super::effects_lookup::runtime_effects(),
        Some(expr.span),
    )
}

/// Promotes and sorts a cell fetched for write after its concrete parent was widened.
fn lower_attached_mixed_cell_key_sort(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    cell: LoweredValue,
    expr: &Expr,
    sort: ArrayKeySort,
) -> Option<LoweredValue> {
    let result = sort_attached_mixed_cell(ctx, name, cell, expr, sort);
    crate::ir_lower::ownership::release_if_owned(ctx, cell, Some(expr.span));
    Some(result)
}

/// Sorts a hash borrowed from a Mixed cell whose owner is already attached to writable storage.
fn sort_attached_mixed_cell(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    cell: LoweredValue,
    expr: &Expr,
    sort: ArrayKeySort,
) -> LoweredValue {
    let hash = promote_attached_mixed_cell_to_hash(ctx, cell, expr, sort);
    super::super::emit_builtin_call_value(
        ctx,
        name,
        vec![hash.value],
        PhpType::Bool,
        expr.span,
        None,
    )
}
