//! Purpose:
//! Lowers `<place> = &$source;` for an object property or array element place
//! (`StmtKind::RefAssignPlace`): the place adopts the source variable's reference cell.
//!
//! Called from:
//! - `crate::ir_lower::stmt::lower_stmt_once()`.
//!
//! Key details:
//! - A PHP reference set is one managed reference cell (heap kind 7). Locals hold it through an
//!   alias slot plus an owner slot, reference properties hold it in their slot, and hash entries
//!   hold it as runtime tag 11. Binding a place means: give the source a cell (promoting a plain
//!   local in place, which keeps its identity), retain that cell into the place, and release
//!   whatever the place held before. Writes through either side then meet in the one cell, and
//!   `unset` of either side only drops that side's owner.
//! - elephc cells are typed. A hash entry cell always carries a boxed `Mixed`, so an element
//!   place widens the source local to `Mixed` storage. A property cell carries the property's
//!   storage representation, so the source must already store that representation (or the
//!   property must be `Mixed`, in which case the local is widened). Any other combination is
//!   refused at compile time: sharing one cell between two representations would reinterpret
//!   the payload on one side.
//! - A source whose cell this frame does not own (a by-reference parameter) is validated at run
//!   time: the backend accepts only a live managed cell and otherwise raises a catchable `Error`
//!   instead of publishing a caller's frame or array-interior address into a longer-lived place.

use super::*;

/// Lowers one `RefAssignPlace` statement.
pub(crate) fn lower_ref_assign_place(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    source: &Expr,
    span: Span,
) {
    let ExprKind::Variable(source_name) = &source.kind else {
        crate::ir_lower::diagnostics::refuse(
            span,
            "Unsupported reference assignment: the source of a property or element reference \
             must be a variable",
        );
        return;
    };
    if super::dynamic_reference_places::dynamic_place_root(target).is_some() {
        super::dynamic_reference_places::lower_dynamic_ref_place(ctx, target, source_name, span);
        return;
    }
    // `$s->p = &$v` on a `stdClass` local: the property lives in the object's property hash,
    // which is exactly the runtime-named path with a constant name.
    if let Some(target) = super::dynamic_reference_places::stdclass_named_place(ctx, target) {
        super::dynamic_reference_places::lower_dynamic_ref_place(ctx, &target, source_name, span);
        return;
    }
    match &target.kind {
        ExprKind::PropertyAccess { object, property } => {
            lower_bind_property(ctx, target, object, property, source_name, span)
        }
        ExprKind::ArrayAccess { array, index } => {
            lower_bind_element(ctx, array, index, source_name, span)
        }
        _ => crate::ir_lower::diagnostics::refuse(
            span,
            "Unsupported reference assignment target: only object properties and array elements \
             can be bound by reference",
        ),
    }
}

/// Lowers `$obj->prop = &$source`.
fn lower_bind_property(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    object: &Expr,
    property: &str,
    source_name: &str,
    span: Span,
) {
    let object_value = lower_expr(ctx, object);
    if let Some(message) = readonly_reference_error(ctx, object_value.value, property) {
        release_owning_receiver_temporary(ctx, object_value, span);
        crate::ir_lower::stmt::lower_throw_access_error(ctx, &message, span);
        return;
    }
    let owned = match ctx.builder.value_php_type(object_value.value).codegen_repr() {
        PhpType::Object(class) => ctx
            .classes
            .get(class.as_str())
            .is_some_and(|info| info.owned_reference_properties.contains(property)),
        _ => false,
    };
    if !owned {
        crate::ir_lower::diagnostics::refuse(
            span,
            &format!(
                "Unsupported reference assignment to property ${property}: the receiver's class \
                 must be known at compile time and the property must not be a by-reference \
                 promoted constructor parameter"
            ),
        );
        return;
    }
    let property_type = property_get_result_type(ctx, object_value.value, property, Op::PropGet, target);
    let Some(cell) = source_reference_cell(ctx, source_name, &property_type, span) else {
        return;
    };
    let data = ctx.intern_string(property);
    ctx.emit_void(
        Op::BindPropRefCell,
        vec![object_value.value, cell.value],
        Some(Immediate::Data(data)),
        Op::BindPropRefCell.default_effects(),
        Some(span),
    );
    release_owning_receiver_temporary(ctx, object_value, span);
}

/// Lowers `$array[index] = &$source` (at any nesting depth).
fn lower_bind_element(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    source_name: &str,
    span: Span,
) {
    let (prepared_array, receiver_aliases) = prepare_transient_addressable_ref_array_receiver(ctx, array);
    let array = prepared_array.as_ref().unwrap_or(array);
    crate::ir_lower::stmt::promote_by_ref_foreach_source(ctx, array, true);
    let array_value = lower_expr(ctx, array);
    let container_type = ctx.builder.value_php_type(array_value.value).codegen_repr();
    let index_value = lower_expr(ctx, index);
    let supported = match &container_type {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_) => true,
        _ => false,
    };
    if !supported {
        crate::ir_lower::diagnostics::refuse(
            span,
            &format!(
                "Unsupported reference assignment to an array element: the container is stored \
                 as {container_type:?}, which cannot hold a reference; elephc binds element \
                 references only in arrays whose elements are mixed"
            ),
        );
        return;
    }
    let Some(cell) = source_reference_cell(ctx, source_name, &PhpType::Mixed, span) else {
        return;
    };
    ctx.emit_void(
        Op::BindArrayElemRefCell,
        vec![array_value.value, index_value.value, cell.value],
        None,
        Op::BindArrayElemRefCell.default_effects(),
        Some(span),
    );
    release_transient_ref_receiver_aliases(ctx, &receiver_aliases, span);
    // The bind BORROWS its key. A computed key is an owned temporary, and a key read from a
    // local whose frame storage widens to `Mixed` (a hidden key local later unset, or any
    // local re-typed afterwards) is materialized by codegen as an owned detached string, so
    // the key is released here; ownership finalization prunes the release when the read
    // stays a borrow. Without it the string leaked once per execution.
    crate::ir_lower::ownership::release_if_owned(ctx, index_value, Some(span));
    // A property read that is not addressable storage arrives as an owning temporary, exactly
    // as in `lower_ref_assign_array_elem`; the bind retains the cell, not the container.
    if matches!(
        ctx.builder.value_defining_op(array_value.value),
        Some(Op::PropGet | Op::DynamicPropGet | Op::NullsafePropGet)
    ) && ctx.value_is_owning_temporary(array_value)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, array_value, Some(span));
    }
}

/// Gives `name` a managed reference cell whose payload is stored as `place_type`, and returns
/// the cell pointer.
///
/// A plain local is promoted in place (widened to `Mixed` first when the place is `Mixed`); an
/// undefined one is first defined as null, like PHP's `&$v`. A local that is already part of a
/// reference set keeps its cell, which must already have the place's representation.
pub(super) fn source_reference_cell(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    place_type: &PhpType,
    span: Span,
) -> Option<LoweredValue> {
    let place_repr = place_type.codegen_repr();
    if is_superglobal_name(name) || ctx.local_uses_global_storage(name) {
        crate::ir_lower::diagnostics::refuse(
            span,
            &format!(
                "Unsupported reference assignment: ${name} lives in program-global storage, \
                 which has no reference cell a property or element can share"
            ),
        );
        return None;
    }
    if ctx.is_borrowed_element_ref_local(name) {
        crate::ir_lower::diagnostics::refuse(
            span,
            &format!(
                "Unsupported reference assignment: ${name} aliases the inline storage of a typed \
                 array element, which has no reference cell a property or element can share"
            ),
        );
        return None;
    }
    if ctx.is_ref_bound_local(name) {
        let local_repr = ctx.local_type(name).codegen_repr();
        if local_repr != place_repr {
            refuse_representation_mismatch(name, &local_repr, &place_repr, span);
            return None;
        }
    } else {
        let defined = ctx.local_slots.contains_key(name)
            && !matches!(ctx.local_type(name), PhpType::Void);
        if !defined {
            let null = ctx.emit_value(
                Op::ConstNull,
                Vec::new(),
                None,
                PhpType::Void,
                Op::ConstNull.default_effects(),
                Some(span),
            );
            let boxed = ctx.emit_value(
                Op::MixedBox,
                vec![null.value],
                None,
                PhpType::Mixed,
                Op::MixedBox.default_effects(),
                Some(span),
            );
            ctx.store_local(name, boxed, PhpType::Mixed, Some(span));
        }
        if place_repr == PhpType::Mixed {
            ctx.promote_local_mixed_ref_cell(name, Some(span));
        } else if ctx.local_type(name).codegen_repr() == place_repr {
            ctx.promote_local_ref_cell(name, Some(span));
        } else {
            refuse_representation_mismatch(name, &ctx.local_type(name).codegen_repr(), &place_repr, span);
            return None;
        }
    }
    let slot = *ctx.local_slots.get(name)?;
    let immediate = match ctx.ref_cell_owner_slot(name) {
        Some(owner) => Immediate::LocalSlotPair { first: slot, second: owner },
        None => Immediate::LocalSlot(slot),
    };
    Some(ctx.emit_value(
        Op::LoadLocalRefCellPtr,
        Vec::new(),
        Some(immediate),
        PhpType::Pointer(None),
        Op::LoadLocalRefCellPtr.default_effects(),
        Some(span),
    ))
}

/// Reports a source whose storage cannot share one typed cell with the place.
fn refuse_representation_mismatch(name: &str, local: &PhpType, place: &PhpType, span: Span) {
    crate::ir_lower::diagnostics::refuse(
        span,
        &format!(
            "Unsupported reference assignment: ${name} is stored as {local:?} but the place it \
             would share a reference with is stored as {place:?}; elephc references carry one \
             storage type, so both sides must agree (declare them with the same type, or leave \
             the property untyped)"
        ),
    );
}

thread_local! {
    /// The checker's `escaping_ref_params` for the current lowering run.
    static ESCAPING_REF_PARAMS: std::cell::RefCell<std::collections::HashSet<(String, usize)>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Installs the by-reference parameters some body binds into a property or element.
pub(crate) fn install_escaping_ref_params(params: &std::collections::HashSet<(String, usize)>) {
    ESCAPING_REF_PARAMS.with(|cell| *cell.borrow_mut() = params.clone());
}

/// Whether a by-reference parameter with this name and position may be bound into a property or
/// element by its callee, so a local argument must be lent as a managed cell.
pub(super) fn param_escapes_by_reference(name: &str, index: usize) -> bool {
    ESCAPING_REF_PARAMS.with(|cell| cell.borrow().contains(&(name.to_string(), index)))
}

thread_local! {
    /// The checker's `static_local_declared_types` for the current lowering run.
    static STATIC_LOCAL_DECLARED_TYPES: std::cell::RefCell<
        std::collections::HashMap<(String, String), PhpType>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Installs the declared types the checker settled on for written array `static` locals.
pub(crate) fn install_static_local_declared_types(
    types: &std::collections::HashMap<(String, String), PhpType>,
) {
    STATIC_LOCAL_DECLARED_TYPES.with(|cell| *cell.borrow_mut() = types.clone());
}

/// The checker's declared type for the written array `static $name` in `scope`, if any.
pub(crate) fn static_local_declared_type_for(scope: &str, name: &str) -> Option<PhpType> {
    STATIC_LOCAL_DECLARED_TYPES
        .with(|cell| cell.borrow().get(&(scope.to_string(), name.to_string())).cloned())
}

/// Whether `name` is one of PHP's superglobals, which elephc keeps in program-global storage.
fn is_superglobal_name(name: &str) -> bool {
    matches!(
        name,
        "_SERVER" | "_GET" | "_POST" | "_COOKIE" | "_FILES" | "_ENV" | "_REQUEST" | "_SESSION" | "GLOBALS"
    )
}
