//! Purpose:
//! Reference assignment and instance or static property reads.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers an object property read.
pub(super) fn lower_property_get(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    lower_property_get_from_value(ctx, object, property, op, expr)
}

/// Lowers `$target = &$obj->prop`: binds the local `$target` to the reference cell
/// stored in the object's reference-property slot, so reads/writes of either side go
/// through the same cell (write-through). The property was promoted to a reference
/// property by the checker, so its slot holds a live cell pointer.
pub(crate) fn lower_ref_assign_property(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::PropertyAccess { object, property } = &source.kind else {
        return;
    };
    let object = lower_expr(ctx, object);
    let receiver_repr = ctx.builder.value_php_type(object.value).codegen_repr();
    let dynamic_receiver = matches!(receiver_repr, PhpType::Mixed | PhpType::Union(_));
    // A receiver lowered as `Mixed` may still be one class to the checker (an untyped parameter
    // inferred from its call sites), which then promoted that class's cell at its own payload
    // type. The alias takes the payload every promoted cell of this name shares, and the guarded
    // load raises an `Error` for any runtime class whose cell stores another representation:
    // reading it as `Mixed` wrote a box pointer into a raw `int` cell.
    let value_type = match dynamic_receiver.then(|| shared_reference_cell_payload(ctx, property)) {
        Some(Some(shared)) => shared,
        _ => property_get_result_type(ctx, object.value, property, Op::PropGet, source),
    };
    // A stdClass property's cell is held by its hash entry, which a later rebinding
    // (`$o->{$k} = &$other`) drops: the alias needs its own share, as a php reference has.
    // So is an undeclared name on a declared class with a dynamic-property hash: its cell is
    // that hash's entry (`__rt_object_hash_prop_ref_cell`), not a slot the object owns.
    let owns_cell = match receiver_repr {
        PhpType::Object(class) => crate::types::checker::builtin_stdclass::is_stdclass(&class)
            || ctx.classes.get(class.trim_start_matches('\\')).is_some_and(|info| {
                info.owned_reference_properties.contains(property)
                    || (info.dynamic_property_hash_is_name_addressable()
                        && info.visible_property(property).is_none())
            }),
        _ => false,
    };
    let data = ctx.intern_string(property);
    let cell_ptr = if dynamic_receiver {
        ctx.emit_value(
            Op::LoadPropRefCellChecked,
            vec![object.value],
            Some(Immediate::Data(data)),
            value_type.clone(),
            Op::LoadPropRefCellChecked.default_effects(),
            Some(span),
        )
    } else {
        ctx.emit_value(
            Op::LoadPropRefCell,
            vec![object.value],
            Some(Immediate::Data(data)),
            PhpType::Pointer(None),
            Op::LoadPropRefCell.default_effects(),
            Some(span),
        )
    };
    if owns_cell {
        ctx.bind_owned_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
        release_owning_receiver_temporary(ctx, object, span);
    } else {
        ctx.bind_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
    }
}

/// Returns the payload type every class's reference cell for `$property` stores, when they agree.
fn shared_reference_cell_payload(ctx: &LoweringContext<'_, '_>, property: &str) -> Option<PhpType> {
    let mut shared: Option<PhpType> = None;
    for info in ctx.classes.values() {
        if !info.reference_properties.contains(property) {
            continue;
        }
        let Some((_, ty)) = info.properties.iter().find(|(name, _)| name == property) else {
            continue;
        };
        match &shared {
            None => shared = Some(ty.clone()),
            Some(seen) if seen.reference_payload_compatible(ty) => {}
            Some(_) => return None,
        }
    }
    shared
}

/// Binds a local alias directly to a native static property's process-lifetime storage slot.
///
/// The static symbol owns its payload, so the alias itself is borrowed. Container replacements
/// written through the alias are immediately visible through `C::$property`, and the same alias
/// gives `IterStart` a local origin that can reload the replacement after hash growth.
pub(crate) fn lower_ref_assign_static_property(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::StaticPropertyAccess { receiver, property } = &source.kind else {
        return;
    };
    let data = crate::ir_lower::stmt::static_property_ref_data(ctx, receiver, property);
    let value_type = static_property_result_type(ctx, receiver, property, source);
    let cell_ptr = ctx.emit_value(
        Op::LoadStaticPropertyRefCell,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Pointer(None),
        Op::LoadStaticPropertyRefCell.default_effects(),
        Some(span),
    );
    ctx.bind_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
}

/// Lowers a local reference alias to a runtime-named declared property cell.
pub(crate) fn lower_ref_assign_dynamic_property(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::DynamicPropertyAccess { object, property } = &source.kind else {
        return;
    };
    let object = lower_expr(ctx, object);
    let property = lower_expr(ctx, property);
    let property = coerce_to_string_at_span(ctx, property, Some(span));
    let value_type = ctx
        .dynamic_ref_local_types
        .get(&(ctx.loop_storage_scope.clone(), target.to_string()))
        .or_else(|| ctx.local_types.get(target))
        .cloned()
        .unwrap_or(PhpType::Mixed);
    let cell_ptr = ctx.emit_value(
        Op::DynamicPropRefCell,
        vec![object.value, property.value],
        None,
        value_type.clone(),
        Op::DynamicPropRefCell.default_effects(),
        Some(span),
    );
    // The alias takes its own share of the cell, as a php reference does: a stdClass property
    // rebound later (`$o->{$k} = &$other`) drops the hash's share, and the alias must outlive it.
    ctx.bind_owned_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
    if ctx.value_is_owning_temporary(property) {
        crate::ir_lower::ownership::release_if_owned(ctx, property, Some(span));
    }
    // A narrowed receiver (`$data instanceof stdClass` on a mixed `$data`) loads as an owned
    // unboxed object; it is retired once the alias holds its share of the cell, as a property
    // read retires it.
    if ctx.value_is_owning_temporary(object) {
        crate::ir_lower::ownership::release_if_owned(ctx, object, Some(span));
    }
}

/// Lowers `$target = &call()`: binds `$target` to the reference cell returned by a
/// by-reference-returning callee.
///
/// The staging that adopts the transferred cell is declared and PUBLISHED in the unwind chain
/// before the source expression is lowered. That ordering is what makes the lease survive a
/// same-frame catch: the record nests outside every root the call's own arguments publish, so it
/// is still live while the callee's argument temporaries are retired, while an owning receiver is
/// destroyed, and while this function retires whatever `$target` was bound to before. Any of
/// those steps can run a destructor that throws, and none of them may leave the lease held with
/// nothing to release it.
///
/// Only a call whose SELECTED lowering actually returns a raw cell can be bound, which
/// `finish_reference_return_call` reports by adopting into that staging. A statically declared
/// by-reference signature is NOT sufficient on its own, because a call that reaches a dynamic
/// descriptor invoker gets an ordinary owned `Mixed` copy back and the invoker retires the cell
/// (`codegen::runtime_callable_invoker::reference_return`). Binding that value as a cell pointer
/// would dereference a payload word as an address, so an unadopted result is refused with a
/// compile diagnostic instead, and the target is bound to its own managed cell so the remaining
/// lowering stays well formed.
pub(crate) fn lower_ref_assign_call(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    match compiled_call_returns_reference(ctx, source) {
        Some(true) => {}
        Some(false) => {
            lower_ref_assign_of_value_call(ctx, target, source, span, true);
            return;
        }
        None => {
            lower_ref_assign_of_value_call(ctx, target, source, span, false);
            return;
        }
    }
    let (staged, owner) = ctx.predeclare_returned_ref_cell_staging();
    register_owned_call_operand(ctx, owner, span);
    let previous = ctx
        .reference_call_context
        .replace(crate::ir_lower::context::ReferenceCallContext {
            depth: ctx.expression_depth + 1,
            staged: staged.clone(),
            adopted: false,
        });
    let result = lower_expr(ctx, source);
    let adopted = ctx
        .reference_call_context
        .take()
        .is_some_and(|context| context.adopted);
    ctx.reference_call_context = previous;
    if !adopted {
        // The staging slot stayed zero, so detaching its record releases nothing.
        unregister_owned_call_operand(ctx, owner, span);
        crate::ir_lower::diagnostics::refuse(
            span,
            "Unsupported reference assignment: this compiler transfers a reference only from a \
             call it resolves to a by-reference-returning function, method, static method or \
             closure. PHP also allows the reference here, but the selected lowering hands back a \
             copied value rather than the callee's reference cell",
        );
        let value_type = ctx.builder.value_php_type(result.value);
        ctx.store_local(target, result, value_type, Some(span));
        ctx.promote_local_ref_cell(target, Some(span));
        return;
    }
    // Publish the new alias before the old payload's last release can throw. The extra old
    // owner delays that destructor without changing the existing rebinding/owner primitives.
    let previous_owner = super::reference_returns::root_previous_reference_binding(ctx, target, span);
    ctx.alias_local_ref_cell(target, &staged, Some(span));
    if let Some(previous_owner) = previous_owner {
        super::reference_returns::retire_previous_reference_binding(ctx, previous_owner, span);
    }
    // The target now owns the returned cell, including when old-payload retirement threw.
    unregister_owned_call_operand(ctx, owner, span);
    ctx.release_ref_cell_owner(&staged, Some(span));
    // `function &grab(&$x) { return $x; }` transfers the parameter's storage. When that
    // parameter was an object property, the call lowered through a hidden temp, so the
    // returned cell aliases the temp rather than the property. Rebind the target to the
    // property place itself so later writes through `$target` reach `$obj->prop`.
    if let Some(property) = aliased_property_argument(ctx, source) {
        lower_ref_assign_property(ctx, target, &property, span);
    }
}

/// Returns a property argument whose by-reference storage the callee provably returns.
fn aliased_property_argument(
    ctx: &LoweringContext<'_, '_>,
    source: &Expr,
) -> Option<Expr> {
    let ExprKind::FunctionCall { name, args } = &source.kind else {
        return None;
    };
    let signature = ctx.functions.get(name.as_str())?;
    if !signature.by_ref_return {
        return None;
    }
    let alias = ctx.return_alias_summaries.function(name.as_str())?;
    for (index, arg) in args.iter().enumerate() {
        let place = match &arg.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => arg,
        };
        if matches!(place.kind, ExprKind::PropertyAccess { .. })
            && alias.proven_aliases_parameter(index)
        {
            return Some(place.clone());
        }
    }
    None
}

/// Answers whether a call returns a reference CELL: `Some(true)` for a compiled by-reference
/// callee, `Some(false)` for a compiled by-value one, `None` when no compiled body answers.
///
/// Only a by-reference compiled callee returns a cell pointer. A by-value one -- and any call the
/// eval bridge or a dynamic dispatch answers, which yields a boxed value -- was bound as if its
/// result were a cell: the next access read the box's tag word as the cell's content and faulted
/// (`$v = &$obj->byValue(); $v[] = 1;`, or `&$evalObject->ref()`). Closures and callables keep
/// the cell path: their by-reference forms are resolved where they are lowered.
fn compiled_call_returns_reference(ctx: &LoweringContext<'_, '_>, source: &Expr) -> Option<bool> {
    let signature = match &source.kind {
        ExprKind::FunctionCall { name, .. } => {
            lookup_folded_name(ctx.functions.keys(), name.as_str().trim_start_matches('\\'))
                .and_then(|function| ctx.functions.get(&function))
        }
        ExprKind::MethodCall { object, method, .. } => instance_callable_object_class(ctx, object)
            .filter(|class_name| ctx.classes.contains_key(class_name.as_str()))
            .and_then(|class_name| {
                class_method_signature(ctx, &class_name, &php_symbol_key(method))
            }),
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            static_method_implementation_signature(ctx, receiver, method)
        }
        _ => return Some(true),
    };
    signature.map(|signature| signature.by_ref_return)
}

/// Lowers `$target = &call()` as an ordinary assignment of the returned value, the way php runs a
/// by-value callee (after its notice, when `notice` says the callee is known to be one).
fn lower_ref_assign_of_value_call(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
    notice: bool,
) {
    let assignment = Expr::new(
        ExprKind::Assignment {
            target: Box::new(Expr::new(ExprKind::Variable(target.to_string()), span)),
            value: Box::new(source.clone()),
            result_target: None,
            prelude: Vec::new(),
            conditional_value_temp: None,
        },
        span,
    );
    let written = lower_expr(ctx, &assignment);
    crate::ir_lower::ownership::release_if_owned(ctx, written, Some(span));
    if !notice {
        return;
    }
    let message = ctx.intern_string("Notice: Only variables should be assigned by reference\n");
    ctx.emit_void(
        Op::Warn,
        Vec::new(),
        Some(Immediate::Data(message)),
        Op::Warn.default_effects(),
        Some(span),
    );
}

/// Lowers `$target =& $arr[idx]` using the ownership represented by the container.
///
/// The addressable receiver is first represented by a local alias and normalized to hash storage.
/// Its entry then owns a managed tag-11 cell, so the new local can outlive replacement or
/// destruction of the parent. This also gives copy-on-write and growth a writable place where
/// they can publish a replacement reached through a static property or nested element.
pub(crate) fn lower_ref_assign_array_elem(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::ArrayAccess { array, index } = &source.kind else {
        return;
    };
    if instance_object_class_for_ir(ctx, array)
        .is_some_and(|class| class.eq_ignore_ascii_case("WeakMap"))
    {
        let call = Expr::new(ExprKind::MethodCall {
            object: array.clone(), method: "__elephc_reference".into(), args: vec![(**index).clone()],
        }, source.span);
        lower_ref_assign_call(ctx, target, &call, span);
        return;
    }
    let prepared_array = prepare_addressable_ref_array_receiver(ctx, array);
    let array = prepared_array.as_ref().unwrap_or(array);
    crate::ir_lower::stmt::promote_by_ref_foreach_source(ctx, array, true);
    let array_value = lower_expr(ctx, array);
    let array_value = prepare_local_array_element_reference_container(
        ctx,
        array,
        array_value,
        span,
    );
    if binds_element_as_shared_reference(ctx, array, array_value) {
        let key = lower_expr(ctx, index);
        let cell = ctx.emit_value(
            Op::ElemRefCell,
            vec![array_value.value, key.value],
            None,
            PhpType::Mixed,
            Op::ElemRefCell.default_effects(),
            Some(span),
        );
        crate::ir_lower::stmt::release_persisted_string_operand(ctx, key, span);
        ctx.bind_owned_ref_cell_ptr(target, cell, Some(span));
        return;
    }
    let container_type = ctx.builder.value_php_type(array_value.value).codegen_repr();
    let mut index_value = lower_expr(ctx, index);
    let value_type = match &container_type {
        PhpType::Array(elem_ty) => {
            if elem_ty.codegen_repr() == PhpType::Mixed {
                PhpType::Mixed
            } else {
                index_value = coerce_array_key_to_int_at_span(ctx, index_value, Some(index.span), false);
                normalize_value_php_type((**elem_ty).clone())
            }
        }
        PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_) => PhpType::Mixed,
        _ => array_access_result_type(ctx, array_value.value, Op::ArrayGet, source),
    };
    let cell_ptr = ctx.emit_value(
        Op::LoadArrayElemRefCell,
        vec![array_value.value, index_value.value],
        None,
        value_type.clone(),
        Op::LoadArrayElemRefCell.default_effects(),
        Some(span),
    );
    ctx.bind_owned_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
    // A property read that is not addressable storage (a `mixed` property, so no synthetic
    // alias was reified above) arrives as an OWNING temporary -- `PropGet` acquires -- and
    // nothing else releases it: the binding above retains the entry's managed cell, not the
    // container, so the container's owner was simply leaked, one boxed array per
    // `$r = &$obj->m[k]`. Only the property reads are released here. A concrete container
    // loaded from a Mixed-widened local also counts as an owning temporary, but that detached
    // owner is what the backend's consuming store-back transfers, and releasing it too freed
    // the table under the local.
    if matches!(
        ctx.builder.value_defining_op(array_value.value),
        Some(Op::PropGet | Op::DynamicPropGet | Op::NullsafePropGet)
    ) && ctx.value_is_owning_temporary(array_value)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, array_value, Some(span));
    }
}

/// Returns whether `$target = &$array[$k]` makes the element a shared reference in place.
///
/// A local container whose elements are boxed takes PHP's model directly: the element becomes a
/// reference marker over a refcounted cell (`Op::ElemRefCell`) and the local owns a share of that
/// cell. Growing or copying the array then leaves the binding valid, writes through the array
/// reach the local, and `$b[$j] = &$target` joins the same reference. Property and static
/// containers keep the address-based ops until their receivers are republished the same way.
fn binds_element_as_shared_reference(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
    array_value: LoweredValue,
) -> bool {
    let ExprKind::Variable(name) = &array.kind else {
        return false;
    };
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    match ctx.builder.value_php_type(array_value.value).codegen_repr() {
        PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        // A boxed local: the element becomes a reference inside the array its box holds.
        PhpType::Mixed | PhpType::Union(_) => true,
        _ => false,
    }
}

/// Widens a local array's element storage before exposing one element as a writable reference.
///
/// A PHP reference may later receive a value of any type. Concrete indexed/hash element layouts
/// cannot represent such a write safely, so a plain local source is converted to Mixed-valued
/// storage and written back before its element address is taken. Containers already using Mixed
/// slots and non-local receiver shapes are left unchanged for their dedicated place lowerers.
pub(crate) fn prepare_local_array_element_reference_container(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    array_value: LoweredValue,
    span: Span,
) -> LoweredValue {
    let ExprKind::Variable(name) = &array.kind else {
        return array_value;
    };
    let container_type = ctx.builder.value_php_type(array_value.value).codegen_repr();
    let (op, target_type) = match container_type {
        PhpType::Array(element) if element.codegen_repr() != PhpType::Mixed => (
            Op::ArrayToMixed,
            PhpType::Array(Box::new(PhpType::Mixed)),
        ),
        PhpType::AssocArray { key, value } if value.codegen_repr() != PhpType::Mixed => (
            Op::HashToMixed,
            PhpType::AssocArray {
                key,
                value: Box::new(PhpType::Mixed),
            },
        ),
        _ => return array_value,
    };
    let converted = ctx.emit_value(
        op,
        vec![array_value.value],
        None,
        target_type.clone(),
        op.default_effects(),
        Some(span),
    );
    ctx.store_mutated_local(name, converted, target_type, Some(span));
    ctx.load_local(name, Some(span))
}

/// Reifies a static property, stable declared property, or nested element as a local receiver.
///
/// Element reference lowering can only republish COW or growth through `ReceiverPlace` when its
/// receiver comes from a local or ref-cell slot. Each synthetic alias names the original storage,
/// and recursively normalizing element parents to hash storage gives every level a managed cell.
pub(crate) fn prepare_addressable_ref_array_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    source: &Expr,
) -> Option<Expr> {
    prepare_addressable_ref_array_receiver_impl(ctx, source, None)
}

/// Prepares a nested ref-argument receiver and records its managed expression aliases.
pub(crate) fn prepare_scoped_addressable_ref_array_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    source: &Expr,
) -> Option<(Expr, Vec<String>)> {
    let mut aliases = Vec::new();
    let receiver = prepare_addressable_ref_array_receiver_impl(ctx, source, Some(&mut aliases))?;
    Some((receiver, aliases))
}

/// Creates reference-bound aliases for nested array receivers, preserving COW and optional call-scoped leases.
fn prepare_addressable_ref_array_receiver_impl(
    ctx: &mut LoweringContext<'_, '_>,
    source: &Expr,
    mut scoped_aliases: Option<&mut Vec<String>>,
) -> Option<Expr> {
    match &source.kind {
        ExprKind::Variable(_) => Some(source.clone()),
        ExprKind::PropertyAccess { object, property }
            if by_ref_foreach_property_source_is_addressable(ctx, object, property) =>
        {
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            lower_ref_assign_property(ctx, &alias, source, source.span);
            // This alias exists only to make the property's storage addressable for a nested
            // reference. Detach an exact PHP array zval before the child promotion can update it
            // in place, while stores through the ref-bound local still publish into the property.
            crate::ir_lower::stmt::load_array_local_for_write(ctx, &alias, source.span);
            if let Some(aliases) = scoped_aliases.as_deref_mut() {
                if publish_scoped_ref_receiver_alias(ctx, &alias, source.span) {
                    aliases.push(alias.clone());
                }
            }
            Some(Expr::new(ExprKind::Variable(alias), source.span))
        }
        ExprKind::StaticPropertyAccess { .. } => {
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            lower_ref_assign_static_property(ctx, &alias, source, source.span);
            // Static array values are boxed. Clone the zval through the synthetic ref-bound
            // alias so an earlier by-value copy keeps its own cell while nested writes publish
            // the detached cell back into the process-lifetime static slot.
            crate::ir_lower::stmt::load_array_local_for_write(ctx, &alias, source.span);
            if let Some(aliases) = scoped_aliases.as_deref_mut() {
                if publish_scoped_ref_receiver_alias(ctx, &alias, source.span) {
                    aliases.push(alias.clone());
                }
            }
            Some(Expr::new(ExprKind::Variable(alias), source.span))
        }
        ExprKind::ArrayAccess { array, index } => {
            let call_scoped = scoped_aliases.is_some();
            let parent = prepare_addressable_ref_array_receiver_impl(
                ctx,
                array,
                scoped_aliases.as_deref_mut(),
            )?;
            let element = Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(parent),
                    index: index.clone(),
                },
                source.span,
            );
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            lower_ref_assign_array_elem(ctx, &alias, &element, source.span);
            // A persistent `=&` alias must detach a child still shared with a by-value snapshot
            // before a deeper reference binds one of its slots. A call-scoped argument does not:
            // the callee's mutating ref-cell path separates and republishes the payload, while a
            // second argument naming the same element must retain that exact cell. Cloning here
            // would replace the child between the two leases and split one PHP reference set.
            if !call_scoped {
                crate::ir_lower::stmt::load_array_local_for_write(ctx, &alias, source.span);
            }
            if let Some(aliases) = scoped_aliases.as_deref_mut() {
                if publish_scoped_ref_receiver_alias(ctx, &alias, source.span) {
                    aliases.push(alias.clone());
                }
            }
            Some(Expr::new(ExprKind::Variable(alias), source.span))
        }
        _ => None,
    }
}

/// Binds a synthetic foreach source alias to an element after its receiver was promoted to hash.
///
/// Unlike an ordinary indexed `=&` binding, a promoted hash entry owns a managed reference cell.
/// Retaining that cell gives the synthetic origin a stable writeback address across table growth,
/// source replacement, and destruction of the enclosing container.
pub(crate) fn lower_owned_ref_assign_array_elem(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::ArrayAccess { array, index } = &source.kind else {
        return;
    };
    let array_value = lower_expr(ctx, array);
    let index_value = lower_expr(ctx, index);
    let value_type = match ctx.builder.value_php_type(array_value.value).codegen_repr() {
        PhpType::Array(elem_ty) => normalize_value_php_type(*elem_ty),
        PhpType::AssocArray { value, .. } => normalize_value_php_type(*value),
        _ => PhpType::Mixed,
    };
    let cell_ptr = ctx.emit_value(
        Op::LoadArrayElemRefCellExisting,
        vec![array_value.value, index_value.value],
        None,
        value_type.clone(),
        Op::LoadArrayElemRefCellExisting.default_effects(),
        Some(span),
    );
    ctx.bind_owned_local_ref_cell_ptr(target, cell_ptr, value_type, Some(span));
}

/// Lowers a named property read once the receiver is already evaluated.
/// Returns whether a read of `$property` on an interface- or `object`-typed receiver can only be
/// answered by `__get`: no class the receiver can be declares the property, and one has `__get`.
/// A class receiver that neither declares `property` nor serves it, where a descendant's `__get`
/// does: `DateTime::modify()` is declared `DateTime` and Carbon reads `->month` off it.
fn class_receiver_reads_through_descendant_magic_get(
    ctx: &LoweringContext<'_, '_>,
    target: &str,
    property: &str,
) -> bool {
    let Some(info) = ctx.classes.get(target) else {
        return false;
    };
    if info.visible_property(property).is_some()
        || info.methods.contains_key("__get")
        || info.dynamic_property_hash_is_name_addressable()
    {
        return false;
    }
    let mut answers_magic = false;
    for (class_name, info) in ctx.classes.iter() {
        if class_name.as_str() == target
            || !crate::types::class_inherits_from(ctx.classes, class_name, target)
        {
            continue;
        }
        if info.visible_property(property).is_some() {
            return false;
        }
        answers_magic |=
            info.methods.contains_key("__get") || info.dynamic_property_hash_is_name_addressable();
    }
    answers_magic
}

fn contract_receiver_reads_through_magic_get(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> bool {
    let PhpType::Object(target) = ctx.builder.value_php_type(object).codegen_repr() else {
        return false;
    };
    let target = target.trim_start_matches('\\');
    let interface = ctx
        .interfaces
        .iter()
        .find(|(name, _)| name.trim_start_matches('\\').eq_ignore_ascii_case(target))
        .map(|(_, info)| info);
    if !target.is_empty() && interface.is_none() {
        return class_receiver_reads_through_descendant_magic_get(ctx, target, property);
    }
    // `CarbonInterface` declares `__get` itself: the run-time object answers even when no
    // implementation is in the compiled world.
    let mut answers_magic = interface.is_some_and(|info| info.methods.contains_key("__get"));
    for (class_name, info) in ctx.classes.iter() {
        if !target.is_empty()
            && !super::array_access_types::class_implements_interface_for_ir(ctx, class_name, target)
        {
            continue;
        }
        if info.visible_property(property).is_some() {
            return false;
        }
        answers_magic |= info.methods.contains_key("__get");
    }
    answers_magic
}

pub(super) fn lower_property_get_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    let object = refine_member_receiver(ctx, object, expr);
    if op == Op::NullsafePropGet && value_is_definitely_null(ctx, object.value) {
        return lower_boxed_null(ctx, expr);
    }
    // A plain read on a receiver that is statically `null` only type-checks inside a null probe
    // (`$null->p ?? $d`, `$null->a->b ?? $d`), where PHP answers null without a warning.
    if op == Op::PropGet && matches!(ctx.builder.value_php_type(object.value), PhpType::Void) {
        return lower_boxed_null(ctx, expr);
    }
    if property_receiver_is_unresolved_nominal_class(ctx, object.value) {
        // PHP declarations may name a class that is absent until a runtime path actually
        // constructs it. Preserve a property read in such a body as a gradual class-id dispatch
        // instead of requiring a static slot layout during AOT lowering.
        let object = ctx.box_value_as_mixed(object, PhpType::Mixed, Some(expr.span));
        let property_expr = Expr::new(ExprKind::StringLiteral(property.to_string()), expr.span);
        return lower_dynamic_property_fetch_from_value(
            ctx,
            object,
            &property_expr,
            PropertyFetchMode::Read,
            expr,
        );
    }
    // A contract receiver (an interface, or plain `object`) whose candidate classes declare no such
    // property but answer undeclared reads through `__get`: the slot ladder has no arm to take, so
    // the read is resolved by name on the run-time object, `__get` included (Carbon's language
    // files read `$date->dayOfWeek` off a `CarbonInterface`).
    if matches!(op, Op::PropGet | Op::NullsafePropGet)
        && contract_receiver_reads_through_magic_get(ctx, object.value, property)
    {
        let object = ctx.box_value_as_mixed(object, PhpType::Mixed, Some(expr.span));
        let property_expr = Expr::new(ExprKind::StringLiteral(property.to_string()), expr.span);
        return lower_dynamic_property_fetch_from_value(
            ctx,
            object,
            &property_expr,
            PropertyFetchMode::Read,
            expr,
        );
    }
    // Route a read of a get-hooked property to its synthetic accessor, except inside that property's
    // own accessor, where `$this->prop` must read the raw backing slot to avoid infinite recursion.
    // A nullsafe read (`$obj?->prop`) routes to a nullsafe call so the null short-circuit is kept.
    if matches!(op, Op::PropGet | Op::NullsafePropGet)
        && class_declares_hook_accessor(ctx, object.value, &property_hook_get_method(property))
        && !ctx.in_own_property_accessor(property)
    {
        let accessor = property_hook_get_method(property);
        let call_op = if op == Op::NullsafePropGet {
            Op::NullsafeMethodCall
        } else {
            Op::MethodCall
        };
        return lower_method_call_with_receiver(ctx, object, &accessor, &[], call_op, expr);
    }
    // A DECLARED property this scope may not read, on a class that declares `__get`: php answers
    // `__get('name')`. As a call the answer is the owned value it is; as a slot read it was
    // treated as borrowed, so every `$entity->private` answered by `__get` leaked its string.
    if matches!(op, Op::PropGet | Op::NullsafePropGet)
        && magic_get_answers_declared_property(ctx, object.value, property)
    {
        let call_op = if op == Op::NullsafePropGet {
            Op::NullsafeMethodCall
        } else {
            Op::MethodCall
        };
        let name = Expr::new(ExprKind::StringLiteral(property.to_string()), expr.span);
        return lower_method_call_with_receiver(ctx, object, "__get", &[name], call_op, expr);
    }
    let data = ctx.intern_string(property);
    let result_type = property_get_result_type(ctx, object.value, property, op, expr);
    let reads_extern_cstr = op == Op::PropGet
        && result_type == PhpType::Str
        && extern_receiver_class(ctx, &ctx.builder.value_php_type(object.value)).is_some();
    let result = ctx.emit_value(
        op,
        vec![object.value],
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    if reads_extern_cstr {
        // A C `char*` field reads as a borrowed view of C memory the program may free or
        // rewrite at any time, so the value PHP keeps is an owned copy taken right here.
        return ctx.emit_owned_value(
            Op::StrPersist,
            vec![result.value],
            None,
            PhpType::Str,
            Op::StrPersist.default_effects(),
            Some(expr.span),
        );
    }
    stabilize_borrowed_result_and_release_receiver(ctx, object, result, expr.span)
}

/// Whether a read of `property` on this receiver is php's `__get('property')`: one known class,
/// which declares the property (an undeclared name keeps its own path, with php's recursion guard
/// inside `__get`) but not visibly to this scope, and declares `__get`.
fn magic_get_answers_declared_property(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> bool {
    let object_type = ctx.builder.value_php_type(object);
    let Some((class_name, _)) = singular_object_class(&object_type) else {
        return false;
    };
    let class_name = class_name.trim_start_matches('\\');
    let Some(class_info) = ctx.classes.get(class_name) else {
        return false;
    };
    class_info.properties.iter().any(|(name, _)| name == property)
        && class_info.methods.contains_key(&php_symbol_key("__get"))
        && !super::native_isset::property_is_accessible_for_ir(ctx, class_name, class_info, property)
}

/// Returns whether a nominal object type has no declaration metadata in this compilation unit.
fn property_receiver_is_unresolved_nominal_class(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
) -> bool {
    let object_type = ctx.builder.value_php_type(object);
    let Some((class_name, _)) = singular_object_class(&object_type) else {
        return false;
    };
    let class_name = class_name.trim_start_matches('\\');
    !class_name.is_empty()
        && !ctx.classes.contains_key(class_name)
        && !ctx.interfaces.contains_key(class_name)
        && !ctx.packed_classes.contains_key(class_name)
}

/// Lowers a direct backing-slot read without invoking a declared property get hook.
pub(super) fn lower_raw_property_get_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    let data = ctx.intern_string(property);
    let result_type = property_get_result_type(ctx, object.value, property, Op::PropGet, expr);
    let result = ctx.emit_value(
        Op::PropGet,
        vec![object.value],
        Some(Immediate::Data(data)),
        result_type,
        Op::PropGet.default_effects(),
        Some(expr.span),
    );
    stabilize_borrowed_result_and_release_receiver(ctx, object, result, expr.span)
}

/// Returns true when value metadata proves the runtime value is PHP null.
pub(super) fn value_is_definitely_null(ctx: &LoweringContext<'_, '_>, value: crate::ir::ValueId) -> bool {
    matches!(ctx.builder.value_php_type(value), PhpType::Void | PhpType::Never)
}

/// Returns true when value metadata permits PHP null at runtime.
pub(super) fn value_is_nullable(ctx: &LoweringContext<'_, '_>, value: crate::ir::ValueId) -> bool {
    match ctx.builder.value_php_type(value) {
        PhpType::Void | PhpType::Never | PhpType::Mixed => true,
        PhpType::Union(members) => members
            .iter()
            .any(|member| matches!(member, PhpType::Void | PhpType::Mixed)),
        _ => false,
    }
}

/// Returns precise PHP metadata for a named property read when class metadata is available.
pub(super) fn property_get_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    op: Op,
    expr: &Expr,
) -> PhpType {
    let slot_ty = property_result_type(ctx, object, property, op, expr, true);
    let Some(flow_ty) = ctx
        .flow_typed_property_accesses
        .get(&(ctx.loop_storage_scope.clone(), expr.span))
    else {
        return slot_ty;
    };
    let flow_ty = normalize_value_php_type(flow_ty.clone());
    // A flow fact narrows what the value IS, never how the slot STORES it. The checker records
    // the property's pre-EIR type, and an untyped array property is normalized to hash storage
    // after checking: `public $items = [2, 4]` is checked as `array<int>` and stored as a hash.
    // Reading that hash as a packed list printed `0,10,1` for `2,4,9` and an address for `12`.
    // A boxed slot narrowed to an OBJECT is what the flow fact exists for (`$this->p instanceof X`
    // then `$this->p->m()`); a boxed slot narrowed to a container is not, and reading the box as
    // the array it holds counted `($box->items[0] = $box->items)` as 4 elements instead of 0.
    let slot_repr = slot_ty.codegen_repr();
    let flow_repr = flow_ty.codegen_repr();
    if flow_repr == slot_repr {
        return flow_ty;
    }
    match slot_repr {
        PhpType::Array(_) | PhpType::AssocArray { .. } => slot_ty,
        PhpType::Mixed | PhpType::Union(_) if !matches!(flow_repr, PhpType::Object(_)) => slot_ty,
        _ => flow_ty,
    }
}

/// Returns the property's slot type, IGNORING the null a receiver that may be a container miss
/// would otherwise add.
///
/// The by-reference `foreach` fetch-for-write read uses this because it never produces that
/// null: PHP evaluates such a source in a WRITE context, where a null receiver is a fatal
/// `Error`, and `PropGetForWrite` raises exactly that instead of answering (issue #690).
pub(super) fn property_slot_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    op: Op,
    expr: &Expr,
) -> PhpType {
    property_result_type(ctx, object, property, op, expr, false)
}

/// Shared body of the two property result-type queries.
///
/// `receiver_miss_is_null` says whether a receiver read that can MISS its container -- an array
/// or hash element -- makes the answer nullable. A read does produce null there; a
/// fetch-for-write does not.
fn property_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    op: Op,
    expr: &Expr,
    receiver_miss_is_null: bool,
) -> PhpType {
    if op == Op::NullsafePropGet {
        return PhpType::Mixed;
    }
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, nullable)) = singular_object_class(&object_ty) else {
        if matches!(object_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            return PhpType::Mixed;
        }
        if let Some(class_name) = extern_receiver_class(ctx, &object_ty) {
            // A C struct field reads as whatever its declared C type widens to: `int` for every
            // integer width, `float` for both float widths.
            let Some(field) = ctx
                .extern_classes
                .get(&class_name)
                .and_then(|info| info.fields.iter().find(|field| field.name == property))
            else {
                return fallback_expr_type(expr);
            };
            return normalize_value_php_type(field.php_type.clone());
        }
        if let Some(class_name) = packed_receiver_class(ctx, &object_ty) {
            let normalized = class_name.trim_start_matches('\\');
            let Some(class_info) = ctx.packed_classes.get(normalized) else {
                return fallback_expr_type(expr);
            };
            let Some(field) = class_info.fields.iter().find(|field| field.name == property) else {
                return fallback_expr_type(expr);
            };
            return normalize_value_php_type(field.php_type.codegen_repr());
        }
        return fallback_expr_type(expr);
    };
    let nullable =
        nullable || (receiver_miss_is_null && value_may_carry_container_miss(ctx, object));
    let normalized = class_name.trim_start_matches('\\');
    if normalized.is_empty() {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    if is_builtin_stdclass_name(normalized) {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    if let Some(property_ty) =
        crate::types::checker::reflection_virtual_property_type_in(normalized, property, |c| {
            ctx.classes.get(c).and_then(|info| info.parent.clone())
        })
    {
        return if nullable {
            nullable_result_type(property_ty)
        } else {
            property_ty
        };
    }
    if let Some(property_ty) = interface_property_result_type(ctx, normalized, property) {
        return if nullable {
            nullable_result_type(property_ty)
        } else {
            property_ty
        };
    }
    let Some(class_info) = ctx.classes.get(normalized) else {
        // An interface that declares no such property in its contract has no slot to type the
        // read from; the implementers' class-id ladder answers it as a boxed value. The syntactic
        // guess (`int`) typed Doctrine's `$entry->class` read as an integer and printed `0`.
        if ctx.interfaces.contains_key(normalized) {
            return PhpType::Mixed;
        }
        return fallback_expr_type(expr);
    };
    let property = crate::types::checker::reflection_virtual_property_backing_in(
        normalized,
        property,
        |c| ctx.classes.get(c).and_then(|info| info.parent.clone()),
    )
    .unwrap_or(property);
    if let Some(property_ty) = runtime_property_type_override(ctx, normalized, property) {
        let property_ty = normalize_value_php_type(property_ty);
        return if nullable {
            nullable_result_type(property_ty)
        } else {
            property_ty
        };
    }
    // `visible_property` still answers for a strict ancestor's PRIVATE slot, which php resolves
    // to a dynamic property everywhere but the class that declared it. Typing the read from that
    // slot handed the backend a non-null-capable result for a name whose answer is the dynamic
    // hash, or php `null`.
    if class_info.visible_property(property).is_some()
        && property_name_is_dynamic_in_scope(ctx, normalized, property)
    {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    if undeclared_read_may_hit_dynamic_storage(class_info, property) {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    if !property_is_accessible_for_ir(ctx, normalized, class_info, property) {
        if let Some(magic_ty) = magic_get_result_type(ctx, normalized) {
            return if nullable {
                nullable_result_type(magic_ty)
            } else {
                magic_ty
            };
        }
    }
    let Some((_, (_, property_ty))) = class_info.visible_property(property) else {
        if let Some(magic_ty) = magic_get_result_type(ctx, normalized) {
            return if nullable {
                nullable_result_type(magic_ty)
            } else {
                magic_ty
            };
        }
        // The clone-override hash answers an undeclared name exactly like the attribute's does,
        // so both reserve the same boxed `mixed` result rather than a declared slot type.
        if class_info.dynamic_property_hash_is_name_addressable() {
            return if nullable {
                nullable_result_type(PhpType::Mixed)
            } else {
                PhpType::Mixed
            };
        }
        // No slot, accessor or hash answers the name on the static class, so the checker
        // narrowed the receiver to a subclass that declares it, and codegen reads it from the
        // runtime class into a boxed result.
        if class_info.properties.iter().all(|(name, _)| name != property) {
            return if nullable {
                nullable_result_type(PhpType::Mixed)
            } else {
                PhpType::Mixed
            };
        }
        return fallback_expr_type(expr);
    };
    let property_ty = normalize_value_php_type(property_ty.clone());
    if nullable {
        nullable_result_type(property_ty)
    } else {
        property_ty
    }
}

/// Returns the packed class a property receiver addresses, or `None` for any other receiver.
///
/// A packed record is reached either as a `packed` value (a buffer element or a nested record
/// field, both of which are the record's address) or through a `ptr<T>` typed pointer whose
/// target is a packed class. Both carry the same raw address, so field reads and writes use the
/// packed layout for either.
pub(crate) fn packed_receiver_class(
    ctx: &LoweringContext<'_, '_>,
    object_ty: &PhpType,
) -> Option<String> {
    match object_ty.codegen_repr() {
        PhpType::Packed(class_name) => Some(class_name),
        PhpType::Pointer(Some(class_name))
            if ctx
                .packed_classes
                .contains_key(class_name.trim_start_matches('\\')) =>
        {
            Some(class_name)
        }
        _ => None,
    }
}

/// Returns the extern class a `ptr<T>` receiver addresses, or `None` for any other receiver.
///
/// The sibling of `packed_receiver_class` for C structs declared with `extern class`: the
/// receiver is the struct's raw address, and each field is read and written at its natural C
/// offset with its declared width.
pub(crate) fn extern_receiver_class(
    ctx: &LoweringContext<'_, '_>,
    object_ty: &PhpType,
) -> Option<String> {
    let PhpType::Pointer(Some(class_name)) = object_ty.codegen_repr() else {
        return None;
    };
    let normalized = class_name.trim_start_matches('\\');
    ctx.extern_classes
        .contains_key(normalized)
        .then(|| normalized.to_string())
}

/// Returns the readable property type guaranteed by an interface contract or enum interface.
fn interface_property_result_type(
    ctx: &LoweringContext<'_, '_>,
    interface_name: &str,
    property: &str,
) -> Option<PhpType> {
    let interface = ctx.interfaces.get(interface_name)?;
    if let Some(property_ty) = interface
        .properties
        .get(property)
        .and_then(|contract| contract.get_type.clone())
    {
        return Some(property_ty);
    }
    if property == "name"
        && interface_extends_interface_for_ir(ctx, interface_name, "UnitEnum")
    {
        return Some(PhpType::Str);
    }
    if property == "value"
        && interface_extends_interface_for_ir(ctx, interface_name, "BackedEnum")
    {
        return Some(PhpType::Union(vec![PhpType::Int, PhpType::Str]));
    }
    None
}

/// Returns whether a container read can carry PHP null in a statically non-null pointer type.
pub(super) fn value_may_carry_container_miss(
    ctx: &LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
) -> bool {
    let Some(inst) = ctx.builder.value_defining_instruction(value) else {
        return false;
    };
    match inst.op {
        Op::ArrayGet | Op::ArrayGetSilent | Op::HashGet | Op::HashGetSilent => true,
        Op::Acquire => inst
            .operands
            .first()
            .copied()
            .is_some_and(|source| value_may_carry_container_miss(ctx, source)),
        _ => false,
    }
}

/// Whether a read of the undeclared `property` may find a dynamic property the object stores
/// itself, read without `__get`: a class that allows dynamic properties and also declares `__get`
/// (guzzle's `FnStream` keeps its `_fn_*` callables that way behind a throwing `__get(): void`).
/// The checker answers such a read `mixed` (`infer_property_on_class_type`), and so must lowering.
pub(in crate::ir_lower) fn undeclared_read_may_hit_dynamic_storage(
    class_info: &crate::types::ClassInfo,
    property: &str,
) -> bool {
    class_info.allow_dynamic_properties
        && class_info.methods.contains_key("__get")
        && class_info.visible_property(property).is_none()
}

/// Returns the normalized return type for a class `__get` magic property hook.
pub(super) fn magic_get_result_type(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Option<PhpType> {
    class_method_signature(ctx, class_name, &php_symbol_key("__get"))
        .map(|signature| normalize_value_php_type(signature.return_type.clone()))
}

/// Adds nullability to a result type without nesting existing union metadata.
pub(super) fn nullable_result_type(php_type: PhpType) -> PhpType {
    match php_type {
        PhpType::Union(mut members) => {
            if !members.iter().any(|member| matches!(member, PhpType::Void)) {
                members.push(PhpType::Void);
            }
            PhpType::Union(members)
        }
        other => PhpType::Union(vec![other, PhpType::Void]),
    }
}

/// Returns true when the runtime class of `object` declares the synthetic property-hook accessor
/// `accessor_method` (`__propget_<p>` / `__propset_<p>`). Drives the decision to route a property
/// read/write to a hook; inherited (flattened) methods count, so subclasses inherit hooks.
pub(super) fn class_declares_hook_accessor(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    accessor_method: &str,
) -> bool {
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, _nullable)) = singular_object_class(&object_ty) else {
        return false;
    };
    let key = php_symbol_key(accessor_method);
    ctx.classes
        .get(class_name)
        .is_some_and(|info| info.methods.contains_key(&key))
}

/// Reads a property already proven initialized, for `isset()`, `empty()` and `??`.
///
/// A receiver that names no single class (a `mixed` value) is resolved by its runtime class, and
/// on that path the ordinary read enforces visibility: `isset($m->private)` raised "Cannot access
/// private property" where php answers false in silence. The probe fetch is the same read without
/// php's access and miss diagnostics; a receiver of one known class keeps the typed slot read.
pub(super) fn lower_initialized_slot_read(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    lower_initialized_slot_read_in_mode(ctx, object, property, PropertyFetchMode::Probe, expr)
}

/// The same read with an explicit silent-fetch mode: `??` and `empty()` pass `Coalesce`, so a
/// receiver whose class declares `__get` is asked for the value (after `__isset`), where `isset()`
/// stays `Probe` and asks `__isset` alone.
pub(super) fn lower_initialized_slot_read_in_mode(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    mode: PropertyFetchMode,
    expr: &Expr,
) -> LoweredValue {
    // A receiver of one known class keeps the typed slot read -- but only for a slot this scope
    // may read. A private or protected one answers through the class's accessors or not at all,
    // which is what the mode-aware fetch does: `$entity->private ?? 'd'` from outside the class
    // read the private value where php asks `__isset`/`__get` (or answers the default).
    let typed_read_allowed = singular_object_class(&ctx.builder.value_php_type(object.value))
        .map(|(class_name, _)| class_name.to_string())
        .is_some_and(|class_name| {
            ctx.classes.get(class_name.as_str()).is_none_or(|class_info| {
                super::native_isset::property_is_accessible_for_ir(
                    ctx,
                    &class_name,
                    class_info,
                    property,
                )
            })
        });
    if typed_read_allowed {
        lower_property_get_from_value(ctx, object, property, Op::PropGet, expr)
    } else {
        lower_property_fetch_mode_from_value(ctx, object, property, mode, expr)
    }
}

/// Whether `static::$property` names a static NO class the call can be bound to declares: the
/// lexical class lacks it and so does every class deriving from it.
///
/// `isset()` of it is then php's constant `false`. Laravel's `HasFactory::newFactory()` probes
/// `isset(static::$factory)` in the copy `App\Models\User` gets, a model declaring none; the
/// read the probe guards is compiled as php's Error by `autoload::runtime_faults`.
pub(super) fn late_bound_static_property_is_absent(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
) -> bool {
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return false;
    };
    let declares = |info: &crate::types::ClassInfo| {
        info.static_properties.iter().any(|(name, _)| name == property)
    };
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return false;
    };
    // A FIXED receiver (`A::`, `self::`, `parent::`) names one class: when neither it nor an
    // ancestor declares the name, `isset(A::$nope)` is php's `false`, where the ordinary read
    // raised `Access to undeclared static property`.
    if !matches!(receiver, StaticReceiver::Static) {
        let mut current = Some(class_info);
        let mut hops = 0usize;
        while let Some(info) = current {
            if declares(info) {
                return false;
            }
            hops += 1;
            if hops > ctx.classes.len() {
                return false;
            }
            current = info.parent.as_deref().and_then(|parent| ctx.classes.get(parent.trim_start_matches('\\')));
        }
        return true;
    }
    !declares(class_info)
        && !ctx.classes.iter().any(|(name, info)| {
            crate::types::class_inherits_from(ctx.classes, name, &class_name) && declares(info)
        })
}

/// Returns true when reading `property` on `receiver` can hit PHP's "must not be accessed
/// before initialization" fatal for a STATIC slot.
///
/// The rule is the instance one: a static property is uninitialized only while it is DECLARED
/// WITH A TYPE. `public static $u;` is plain null from the start, and a receiver whose class
/// is not known statically cannot be probed at all.
///
/// Unlike the instance gate there is no defaulted-slot question to settle here: `unset()` does
/// not apply to a static property, so a default really does make the slot live for good — but
/// asking only "is it typed" costs one sentinel compare on a slot that can never carry the
/// sentinel, and keeps the two gates reading the same way.
pub(super) fn static_property_can_be_uninitialized(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
) -> bool {
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return false;
    };
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return false;
    };
    if !class_info
        .static_properties
        .iter()
        .any(|(name, _)| name == property)
    {
        return false;
    }
    // Whether the slot was DECLARED with a type, asked of the schema rather than inferred from
    // the stored `PhpType`. `public static mixed $s;` is declared AND stores `Mixed`, so a test
    // on the representation read it as untyped and `S::$s ?? "d"` raised where PHP answers the
    // default — the same confusion the instance predicate above had.
    class_info.declared_static_properties.contains(property)
}

/// Reads a static `property` the way `isset()` does: yields null instead of raising when the
/// slot is still uninitialized.
///
/// The instance twin (`lower_initialized_property_value`) branches on `Op::PropInitialized`.
/// The static path had no such operation — its guard is emitted straight into the read — so
/// `S::$s ?? "d"` raised where PHP answers the default. `Op::StaticPropInitialized` is that
/// operation; the probe it lowers to already existed for Reflection and only needed a
/// visibility-enforcing entry point.
///
/// There is no receiver to own or release here, which is the whole difference from the
/// instance form.
pub(super) fn lower_initialized_static_property_value(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    if let Some(error) = crate::ir_lower::property_default_errors::for_receiver(ctx, receiver, expr.span) {
        return error;
    }
    let temp_name = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let uninitialized_block = ctx
        .builder
        .create_named_block("coalesce.static_property.uninitialized", Vec::new());
    let read_block = ctx
        .builder
        .create_named_block("coalesce.static_property.read", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("coalesce.static_property.merge", Vec::new());
    let data = crate::ir_lower::stmt::static_property_ref_data(ctx, receiver, property);
    let initialized = ctx.emit_value(
        Op::StaticPropInitialized,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::StaticPropInitialized.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: read_block,
        then_args: Vec::new(),
        else_target: uninitialized_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(uninitialized_block);
    let null_value = lower_boxed_null(ctx, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Mixed, null_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(read_block);
    let read_value = lower_static_property_get(ctx, receiver, property, expr);
    // Both arms store into one Mixed temporary, so a slot that is not already boxed has to be.
    let read_value = if matches!(
        ctx.builder.value_php_type(read_value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        read_value
    } else {
        ctx.emit_value(
            Op::MixedBox,
            vec![read_value.value],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(expr.span),
        )
    };
    store_value_into_temp(ctx, &temp_name, PhpType::Mixed, read_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Returns the class name and nullability if `php_type` is a single object type (optionally
/// nullable). Heterogeneous unions and non-object types return `None`.
pub(crate) fn singular_object_class(php_type: &PhpType) -> Option<(&str, bool)> {
    match php_type {
        PhpType::Object(name) => Some((name.as_str(), false)),
        PhpType::Union(members) => {
            let mut found = None;
            let mut nullable = false;
            for member in members {
                match member {
                    PhpType::Void => nullable = true,
                    PhpType::Object(name) => {
                        if found.is_some_and(|existing| existing != name.as_str()) {
                            return None;
                        }
                        found = Some(name.as_str());
                    }
                    _ => return None,
                }
            }
            found.map(|class_name| (class_name, nullable))
        }
        _ => None,
    }
}

/// Returns precise runtime storage types for inherited SPL callback-filter internals.
pub(super) fn runtime_property_type_override(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> Option<PhpType> {
    if !class_extends_class(ctx, class_name, "CallbackFilterIterator") {
        return None;
    }
    match property {
        "callback" => Some(PhpType::Callable),
        "callbackEnv" => Some(PhpType::Pointer(None)),
        _ => None,
    }
}

/// Returns true when a class is or extends the target class.
pub(crate) fn class_extends_class(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    target_class: &str,
) -> bool {
    let target_key = php_symbol_key(target_class);
    let mut current = Some(class_name.trim_start_matches('\\').to_string());
    while let Some(name) = current {
        if php_symbol_key(&name) == target_key {
            return true;
        }
        current = ctx
            .classes
            .get(name.as_str())
            .and_then(|class_info| class_info.parent.clone());
    }
    false
}

/// Casts a runtime property name to the `Str` pair the backend property ladders index by.
///
/// PHP resolves `$o->{$e}` through a string cast of `$e`, so any scalar name is legal source.
/// The checker cannot always hand the backend a narrowed one: an `eval()` anywhere in a scope
/// widens every local there to `Mixed`, which is how `$o->{$name}` after `eval('...')` reached
/// codegen boxed and was refused outright. Casting here keeps one string-name contract for the
/// get, set and unset ladders, and leaves an already-`Str` name untouched so its producer stays
/// visible to the owned-temporary cleanup.
pub(super) fn coerce_runtime_property_name(
    ctx: &mut LoweringContext<'_, '_>,
    property: LoweredValue,
    span: Span,
) -> LoweredValue {
    if ctx.builder.value_php_type(property.value).codegen_repr() == PhpType::Str {
        return property;
    }
    let name = ctx.emit_value(
        Op::Cast,
        vec![property.value],
        Some(Immediate::CastTarget(IrType::Str)),
        PhpType::Str,
        Op::Cast.default_effects(),
        Some(span),
    );
    release_coerced_source_if_owned(ctx, property, Some(span));
    name
}

/// Lowers a dynamic property read.
pub(super) fn lower_dynamic_property_get(ctx: &mut LoweringContext<'_, '_>, object: &Expr, property: &Expr, expr: &Expr) -> LoweredValue {
    lower_dynamic_property_fetch(ctx, object, property, PropertyFetchMode::Read, expr)
}

/// Lowers a dynamic property fetch in php's read or silent-probe mode.
pub(super) fn lower_dynamic_property_fetch(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &Expr,
    mode: PropertyFetchMode,
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    lower_dynamic_property_fetch_from_value(ctx, object, property, mode, expr)
}

/// Lowers a dynamic property fetch, in either mode, once the receiver is already evaluated.
///
/// The mode travels on the instruction because php's answer for a name this scope may not reach,
/// or that has never been created, depends on it and on nothing the backend can recover later:
/// a value read raises `Cannot access private property D::$n` or warns `Undefined property`,
/// while `isset()`, `empty()` and `??` answer `null` in silence.
///
/// The mode is NOT an effect narrowing. It suppresses php's own access and miss diagnostics only;
/// a probe still reaches `__isset`, `__get` and property hooks, which are user code that can throw
/// or warn. php 8.5 propagates an exception thrown from `__isset` straight out of `isset()`, so
/// both modes keep the opcode's conservative contract.
pub(super) fn lower_dynamic_property_fetch_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &Expr,
    mode: PropertyFetchMode,
    expr: &Expr,
) -> LoweredValue {
    let object = refine_member_receiver(ctx, object, expr);
    // A literal name on a C struct receiver is the named field read (`$r->{'y'}` is `$r->y`): it
    // keeps the width-aware load and the owned copy of a `char*` field the named read takes. That
    // holds in probe mode too: the checker only admits declared fields, which always exist, so
    // `isset($r->{'y'})`, `empty(...)` and `?? ...` read the field exactly like their named forms.
    if let ExprKind::StringLiteral(name) = &property.kind {
        if extern_receiver_class(ctx, &ctx.builder.value_php_type(object.value)).is_some() {
            return lower_property_get_from_value(ctx, object, name, Op::PropGet, expr);
        }
    }
    let result_type = dynamic_property_get_result_type(ctx, object.value, property, expr);
    // A silent fetch can answer null for a typed slot that has not been initialized. Buffer
    // handles keep their element type and use the raw missing-value sentinel, like buffer
    // element probes: boxing a buffer loses the descriptor's element layout at a later merge.
    let result_type = if mode.is_read() || matches!(result_type, PhpType::Buffer(_)) {
        result_type
    } else {
        nullable_result_type(result_type)
    };
    let property = lower_expr(ctx, property);
    let property = coerce_runtime_property_name(ctx, property, expr.span);
    lower_dynamic_property_fetch_from_operands(ctx, object, property, result_type, mode, expr)
}

/// Fetches a property with an already evaluated name, sharing ordinary getter and ownership rules.
pub(super) fn lower_dynamic_property_fetch_from_operands(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: LoweredValue,
    result_type: PhpType,
    mode: PropertyFetchMode,
    expr: &Expr,
) -> LoweredValue {
    // A class-typed receiver whose read answers a boxed value goes through the shared runtime-name
    // helper the boxed receivers use: inline, its ladder spans the class subtree once per site, and
    // Symfony's `Container::getService()` reads `$this->{$registry}` over the compiled container's
    // properties, 1.86 MB for one method. A body with a runtime closure scope keeps the inline
    // ladder: codegen never outlines its reads, and a boxed receiver would widen it to every class.
    let object = if result_type.codegen_repr() == PhpType::Mixed
        && matches!(ctx.builder.value_php_type(object.value).codegen_repr(), PhpType::Object(_))
        && !ctx.has_local_slot(crate::names::CLOSURE_SCOPE_LOCAL)
    {
        ctx.box_value_as_mixed(object, PhpType::Mixed, Some(expr.span))
    } else {
        box_generic_object_for_dynamic_property(ctx, object, expr.span)
    };
    // A value read CAN THROW (php's access `Error` for a name this scope may not reach). The
    // coerced name and a call-result receiver are owning temporaries, so a caught refusal used
    // to skip both releases below and leak them once per throw. Pinning parks them over the
    // throwing window, as the write and the `unset` do.
    let pins = crate::ir_lower::expr::pin_in_flight_owners(
        ctx,
        &[object.value, property.value],
        expr.span,
    );
    let result = ctx.emit_value(
        Op::DynamicPropGet,
        vec![object.value, property.value],
        Some(Immediate::PropertyFetchMode(mode)),
        result_type,
        Op::DynamicPropGet.default_effects(),
        Some(expr.span),
    );
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, expr.span);
    // A name computed for this read (`$o->{(string) $mixed}`) is a temporary the
    // lookup only reads; the result borrows from the object, never from it.
    if ctx.value_is_owning_temporary(property) {
        crate::ir_lower::ownership::release_if_owned(ctx, property, Some(expr.span));
    }
    stabilize_borrowed_result_and_release_receiver(ctx, object, result, expr.span)
}

/// Lowers `$object->name` as a silent probe by reusing the runtime-name fetch with a literal name.
///
/// `Op::PropGet` already spends its immediate on the property name, so the probe distinction
/// cannot ride along with it. A literal-name `DynamicPropGet` reaches exactly the same backend
/// ladders (`lower_const_dynamic_prop_get` dispatches to the same stdClass, magic, hash and
/// declared-slot lowerings `lower_prop_get_nonnull` does) with the same result type, ownership
/// and effects, so routing the probe through it keeps one set of read semantics.
pub(super) fn lower_property_probe_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    let object = refine_member_receiver(ctx, object, expr);
    // Runtime-name probes must suppress typed uninitialized reads just like coalesce.
    // In particular, isset() on the C|false receiver after a rejected readonly fallback
    // must answer false rather than take DynamicPropGet's ordinary typed-slot guard.
    if property_can_be_uninitialized(ctx, object.value, property)
        || union_hook_probe_class(ctx, object.value, property).is_some() {
        return lower_initialized_property_value(ctx, object, property, expr);
    }
    lower_property_fetch_mode_from_value(ctx, object, property, PropertyFetchMode::Probe, expr)
}

/// Lowers `$object->name` as a silent probe in one explicit fetch mode (`Probe` or `Coalesce`).
pub(super) fn lower_property_fetch_mode_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    mode: PropertyFetchMode,
    expr: &Expr,
) -> LoweredValue {
    let name = Expr::new(ExprKind::StringLiteral(property.to_string()), expr.span);
    lower_dynamic_property_fetch_from_value(ctx, object, &name, mode, expr)
}

/// Like `lower_property_probe_from_value`, for `empty()`: a receiver with `__get` but no
/// `__isset` reports the name absent without calling `__get` (`PropertyFetchMode::Empty`).
pub(super) fn lower_property_empty_probe_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    if property_can_be_uninitialized(ctx, object.value, property)
        || union_hook_probe_class(ctx, object.value, property).is_some() {
        return lower_initialized_property_value(ctx, object, property, expr);
    }
    let name = Expr::new(ExprKind::StringLiteral(property.to_string()), expr.span);
    lower_dynamic_property_fetch_from_value(ctx, object, &name, PropertyFetchMode::Empty, expr)
}

/// Like `lower_property_probe_from_value`, for `isset()`: only the answer's nullness is used,
/// so a receiver answering through magic is asked `__isset` alone (`PropertyFetchMode::Isset`).
pub(super) fn lower_property_isset_probe_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    // As for `??`: a typed slot that may still be uninitialized is probed, never read.
    if property_can_be_uninitialized(ctx, object.value, property)
        || union_hook_probe_class(ctx, object.value, property).is_some() {
        return lower_initialized_property_value(ctx, object, property, expr);
    }
    let name = Expr::new(ExprKind::StringLiteral(property.to_string()), expr.span);
    lower_dynamic_property_fetch_from_value(ctx, object, &name, PropertyFetchMode::Isset, expr)
}

/// Returns whether `$object->name` must be probed through the runtime-name form.
///
/// Only a receiver whose class is unknown at compile time needs it: `property_isset_action`
/// answers for every singular object class, and a name the checker refused never reaches
/// lowering. A boxed `Mixed` receiver has neither, so its declared-slot ladder is the one place
/// where a probe would otherwise take the raising value-read arm.
pub(super) fn property_probe_needs_runtime_name_form(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
) -> bool {
    if isset_object_expr_class(ctx, object).is_some() {
        return false;
    }
    property_probe_needs_runtime_name_form_for_type(&expr_receiver_type_for_probe(ctx, object))
}

/// The receiver-TYPE half of the decision above, for a receiver already lowered to a value.
///
/// A nullable or union receiver that still resolves to one object class keeps the ordinary named
/// read: its declared slot is known and the checker has already ruled on the name.
pub(super) fn property_probe_needs_runtime_name_form_for_type(receiver_ty: &PhpType) -> bool {
    singular_object_class(receiver_ty).is_none()
        && matches!(
            receiver_ty.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
}

/// Returns the lowering-visible PHP type of a probe receiver expression.
fn expr_receiver_type_for_probe(ctx: &LoweringContext<'_, '_>, object: &Expr) -> PhpType {
    match &object.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        _ => infer_expr_type_syntactic(object),
    }
}

/// Boxes a bare `object` receiver so runtime-name property dispatch can inspect its class id.
pub(super) fn box_generic_object_for_dynamic_property(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    span: Span,
) -> LoweredValue {
    // An object typed by an INTERFACE (or by a class this program does not compile) has no class
    // layout to dispatch a runtime name against, so it goes through the runtime-class path a
    // boxed receiver takes. Doctrine's `RawValuePropertyAccessor::setValue()` narrows `$object`
    // to `InternalProxy` before `setRawValue()`, and the backend refused "unknown class".
    let needs_box = match ctx.builder.value_php_type(object.value).codegen_repr() {
        PhpType::Object(class_name) => {
            let name = class_name.trim_start_matches('\\');
            name.is_empty() || !ctx.classes.contains_key(name)
        }
        _ => false,
    };
    if needs_box {
        ctx.box_value_as_mixed(object, PhpType::Mixed, Some(span))
    } else {
        object
    }
}

/// Returns precise metadata for dynamic property reads when class slots are statically known.
pub(super) fn dynamic_property_get_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &Expr,
    expr: &Expr,
) -> PhpType {
    if let ExprKind::StringLiteral(name) = &property.kind {
        return property_get_result_type(ctx, object, name, Op::DynamicPropGet, expr);
    }
    let object_ty = ctx.builder.value_php_type(object);
    if matches!(object_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return PhpType::Mixed;
    }
    let Some((class_name, nullable)) = singular_object_class(&object_ty) else {
        return fallback_expr_type(expr);
    };
    let nullable = nullable || value_may_carry_container_miss(ctx, object);
    let normalized = class_name.trim_start_matches('\\');
    // The bare `object` type names no class, so its runtime-named read can be any property of any
    // class. It fell through to the expression's checker type, `int`, and a string property read
    // through `function f(object $o, string $n) { return $o->$n; }` came back as `1` (Twig's
    // compiled `CoreExtension::getAttribute()` reads every public property this way).
    if normalized.is_empty() || is_builtin_stdclass_name(normalized) {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    let Some(class_info) = ctx.classes.get(normalized) else {
        return fallback_expr_type(expr);
    };
    // A class with a per-instance property hash can answer a name no declaration mentions, so the
    // declared-slot union below would describe the wrong storage: the hash holds boxed `mixed`.
    // Reading the union type instead re-interpreted a boxed cell as the single declared slot type,
    // which printed the null sentinel as an int for `clone($plain, ["zz" => "x"])`.
    // A name this scope resolves to a DYNAMIC property is dropped from the backend's declared-slot
    // ladder, so the runtime name can miss and answer php `null`. Typing the read from the
    // remaining declared slots printed the backend's own miss sentinel as an ordinary value.
    if class_info.dynamic_property_hash_is_name_addressable()
        || class_runtime_name_read_can_miss(ctx, normalized)
    {
        return if nullable {
            nullable_result_type(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
    }
    let members = class_info
        .properties
        .iter()
        .map(|(_, property_ty)| {
            let property_ty = normalize_value_php_type(property_ty.clone());
            if nullable {
                nullable_result_type(property_ty)
            } else {
                property_ty
            }
        })
        .collect::<Vec<_>>();
    normalize_union_members(members).unwrap_or_else(|| fallback_expr_type(expr))
}

/// Returns whether php resolves `property` on `class_name` to a DYNAMIC property in this scope.
///
/// `crate::types::resolve_property_name` is the authority: `ClassInfo::properties` is the
/// PHYSICAL slot table, so it still carries a strict ancestor's private slot under its plain
/// name even though php 7.4 removed shadow properties and the child's by-name table no longer
/// contains it. The lowering scope is the same one `ir_can_access_member` uses.
pub(super) fn property_name_is_dynamic_in_scope(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> bool {
    crate::types::resolve_property_name(
        ctx.classes,
        class_name.trim_start_matches('\\'),
        property,
        ctx.current_class.as_deref(),
    ) == crate::types::PropertyNameResolution::Dynamic
}

/// Returns whether a RUNTIME name on `class_name` can miss every slot the backend will dispatch.
///
/// A name this scope resolves to a dynamic property answers from the per-instance hash, and one
/// php refuses is dropped from a silent probe's ladder. Both reach the ladder's miss arm, whose
/// answer is php `null`, so a result type built only from the remaining declared slots would
/// describe storage the read never produced.
fn class_runtime_name_read_can_miss(ctx: &LoweringContext<'_, '_>, class_name: &str) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    let Some(class_info) = ctx.classes.get(normalized) else {
        return false;
    };
    class_info.properties.iter().any(|(name, _)| {
        !matches!(
            crate::types::resolve_property_name(
                ctx.classes,
                normalized,
                name,
                ctx.current_class.as_deref(),
            ),
            crate::types::PropertyNameResolution::Visible
                | crate::types::PropertyNameResolution::ScopePrivate { .. }
        )
    })
}

/// Returns true when the normalized class name refers to PHP's builtin stdClass.
pub(super) fn is_builtin_stdclass_name(class_name: &str) -> bool {
    crate::types::checker::builtin_stdclass::is_stdclass(class_name)
}

/// Flattens and deduplicates union candidates, with `Mixed` absorbing all members.
pub(super) fn normalize_union_members(members: Vec<PhpType>) -> Option<PhpType> {
    let mut deduped = Vec::new();
    for member in members {
        match member {
            PhpType::Union(inner) => {
                for inner_member in inner {
                    if inner_member == PhpType::Mixed {
                        return Some(PhpType::Mixed);
                    }
                    if !deduped.iter().any(|existing| existing == &inner_member) {
                        deduped.push(inner_member);
                    }
                }
            }
            PhpType::Mixed => return Some(PhpType::Mixed),
            other => {
                if !deduped.iter().any(|existing| existing == &other) {
                    deduped.push(other);
                }
            }
        }
    }
    match deduped.len() {
        0 => None,
        1 => deduped.pop(),
        _ => Some(PhpType::Union(deduped)),
    }
}

/// Lowers a static property read.
pub(super) fn lower_static_property_get(ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, property: &str, expr: &Expr) -> LoweredValue {
    if let Some(error) = crate::ir_lower::property_default_errors::for_receiver(ctx, receiver, expr.span) {
        return error;
    }
    let data = crate::ir_lower::stmt::static_property_ref_data(ctx, receiver, property);
    let result_type = static_property_result_type(ctx, receiver, property, expr);
    ctx.emit_value(
        Op::LoadStaticProperty,
        Vec::new(),
        Some(Immediate::Data(data)),
        result_type,
        Op::LoadStaticProperty.default_effects(),
        Some(expr.span),
    )
}

/// Lowers a static-property read whose property name is computed at runtime.
pub(super) fn lower_dynamic_static_property_get(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let name = lower_expr(ctx, property);
    let name = coerce_to_string_at_span(ctx, name, Some(property.span));
    let class_name =
        static_receiver_class_name(ctx, receiver).unwrap_or_else(|| receiver_name(receiver));
    let data = ctx.intern_string(&class_name);
    let result_type = dynamic_static_property_result_type(ctx, receiver);
    ctx.emit_value(
        Op::LoadDynamicStaticProperty,
        vec![name.value],
        Some(Immediate::Data(data)),
        result_type,
        Op::LoadDynamicStaticProperty.default_effects(),
        Some(expr.span),
    )
}

/// Returns common static-property metadata for a runtime-name read.
fn dynamic_static_property_result_type(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
) -> PhpType {
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return PhpType::Mixed;
    };
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return PhpType::Mixed;
    };
    let mut types = class_info
        .static_properties
        .iter()
        .map(|(_, ty)| normalize_value_php_type(ty.codegen_repr()));
    let Some(first) = types.next() else {
        return PhpType::Mixed;
    };
    if types.all(|ty| ty == first) {
        first
    } else {
        PhpType::Mixed
    }
}

/// Returns precise PHP metadata for a static property read when class metadata is available.
pub(crate) fn static_property_result_type(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    _expr: &Expr,
) -> PhpType {
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return PhpType::Mixed;
    };
    let Some(class_info) = ctx.classes.get(class_name.as_str()) else {
        return PhpType::Mixed;
    };
    let Some((_, property_ty)) = class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
    else {
        return PhpType::Mixed;
    };
    // Preserve a nullable concrete class for fixed-slot property writes. Interface unions keep
    // their boxed runtime dispatch: an instanceof guard can expose another interface's method,
    // which the original declared interface does not own (Store::name, for example).
    let result_type = if singular_object_class(property_ty).is_some_and(|(name, nullable)| {
        nullable && ctx.classes.contains_key(name.trim_start_matches('\\'))
    }) {
        property_ty.clone()
    } else {
        crate::types::eir_value_php_type(property_ty)
    };
    normalize_value_php_type(result_type)
}

/// Returns the slot type of a static property that holds one raw word: a `buffer<T>` handle,
/// a `ptr` or `ptr<T>` address, or a `?ptr<T>` word where null is address 0.
///
/// Merge temps and literal element stamps are predicted before the read is lowered, and the
/// syntactic fallback types every static read `int`, which cast a buffer handle or pointer
/// into an integer slot (a ternary of two static buffers then indexed an `int`). Other static
/// reads answer `None` and keep the typing their callers already had.
pub(crate) fn static_property_word_type(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    expr: &Expr,
) -> Option<PhpType> {
    let loaded = static_property_result_type(ctx, receiver, property, expr);
    matches!(loaded.codegen_repr(), PhpType::Buffer(_) | PhpType::Pointer(_)).then_some(loaded)
}
