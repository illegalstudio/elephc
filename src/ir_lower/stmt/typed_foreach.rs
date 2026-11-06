//! Purpose:
//! Typed assignments and foreach iterator lowering.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.
//! - The loop records body stores of a missing-string sentinel before the body is lowered.

use super::*;

/// Lowers an assignment with a declared type.
pub(super) fn lower_typed_assign(
    ctx: &mut LoweringContext<'_, '_>,
    type_expr: &crate::parser::ast::TypeExpr,
    name: &str,
    value: &Expr,
    span: Span,
) {
    let direct_closure = matches!(value.kind, ExprKind::Closure { .. });
    ctx.clear_pending_static_callable_result();
    let php_type = ctx.type_expr_to_php_type_for_value(type_expr);
    let static_callable = static_callable_binding_for_expr(ctx, value);
    let reflected_class = reflection_class_binding_for_expr(ctx, value);
    let reflected_property = reflection_property_binding_for_expr(ctx, value);
    let fiber_start_sig = crate::ir_lower::fibers::start_sig_for_expr(ctx, value);
    let callable_array = lower_callable_array_for_assignment(ctx, value, static_callable.as_ref());
    let lowered = callable_array
        .as_ref()
        .map(|assignment| assignment.value)
        .unwrap_or_else(|| lower_expr(ctx, value));
    let lowered = crate::ir_lower::expr::coerce_integer_arithmetic(ctx, lowered, &php_type, span);
    // `?ptr<T> $x = null` (or a packed record address) stores the pointer word.
    let lowered = ctx.retype_packed_address_for_pointer_sink(lowered, &php_type, Some(span));
    let lowered = coerce_typed_assign_value(ctx, lowered, &php_type, span);
    let lowered = coerce_declared_array_storage(ctx, lowered, &php_type, span);
    if php_type == PhpType::Int && !ctx.conditional_local_declarations.contains(name) {
        ctx.integer_local_declarations.insert(name.to_owned());
    } else {
        ctx.integer_local_declarations.remove(name);
    }
    ctx.declare_local(name, php_type.clone());
    ctx.store_local(name, lowered, php_type, Some(span));
    let callable_result = if direct_closure {
        ctx.take_pending_static_callable_result()
    } else {
        ctx.clear_pending_static_callable_result();
        None
    };
    let static_callable = callable_array
        .map(|assignment| assignment.target)
        .or(static_callable)
        .or(callable_result);
    if let Some(target) = static_callable {
        ctx.bind_static_callable_local(name, target);
    }
    if let Some(reflected_class) = reflected_class {
        ctx.bind_reflection_class_local(name, reflected_class);
    }
    if let Some((reflected_class, reflected_property)) = reflected_property {
        ctx.bind_reflection_property_local(name, reflected_class, reflected_property);
    }
    if let Some(sig) = fiber_start_sig {
        ctx.bind_fiber_start_sig(name, sig);
    }
}

/// Converts a typed-local array initializer to the element storage its DECLARATION asks for.
///
/// Without this the declaration allocates `array<int>` from `[1,2,3]`, binds the slot as the
/// declared `array<mixed>`, and emits nothing in between, so the element header tag still says
/// int while every later load reads the slot as boxed — `array $a = [1,2,3]; $a[0] = 9;` then
/// segfaulted on a CONFORMING write.
///
/// The INFERRED form was already correct because it has a previous local type for
/// `array_storage_conversion` to compare against; a fresh declaration has none, so the
/// conversion has to be driven from the value's type instead of the slot's history. Same
/// predicate and same op mapping as the region fixpoint, so the two cannot drift.
///
/// Deliberately NOT part of [`coerce_typed_assign_value`], which `lower_property_assign` shares:
/// widening into a declared `array` PROPERTY has its own copy-on-write contract that hands the
/// property a unique converted clone while leaving the caller's source owner alone. Converting
/// there too rewrote the source itself, so `$source = [11,22]; $bag->items = $source;` printed
/// pointers for `$source` while the property still read correctly.
fn coerce_declared_array_storage(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    php_type: &PhpType,
    span: Span,
) -> LoweredValue {
    let target_ty = php_type.codegen_repr();
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if source_ty == target_ty {
        return value;
    }
    let Some(op) = crate::ir_lower::stmt::repr_fixpoint::conversion_op(&source_ty, &target_ty)
    else {
        return value;
    };
    // The conversion takes its operand as an OWNED reference and rewrites the array's storage in
    // place when it is the only owner — that is what makes it free for a fresh literal. A
    // declaration that copies another local (`array $copy = $source;`) hands it a borrowed load
    // instead, so without a reference of its own it rewrote `$source`: its elements came back as
    // Mixed cells while its slot still read them as ints, and `echo $source[0]` printed a pointer.
    //
    // Acquiring first is what makes `__rt_array_ensure_unique` inside the conversion copy rather
    // than convert in place, which is the same contract that already keeps the source intact when
    // the destination is a declared `array` PROPERTY. A fresh temporary already owns itself and is
    // converted directly, so the literal form allocates nothing extra.
    //
    // `value_is_owning_temporary` is the wrong question to ask here: it answers yes for a plain
    // `load_local` of a user local, because it describes what a CONSUMER may release, not who
    // holds the slot's reference. The operand's own ownership is the discriminator — a fresh
    // temporary is `Owned`, a local read is `MaybeOwned`, and only the latter needs a reference
    // taken before the conversion consumes one.
    let value = if ctx.builder.value_ownership(value.value) == Ownership::Owned {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
    };
    ctx.emit_value(
        op,
        vec![value.value],
        None,
        target_ty,
        op.default_effects(),
        Some(span),
    )
}

/// Coerces a value stored into a declared PROPERTY slot (instance or static).
///
/// Identical to `coerce_typed_assign_value`, except that a `Closure`-typed slot fed from a
/// `Mixed` value takes a guarded unbox: php raises a TypeError for `self::$make = 5` on a
/// `\Closure $make`, and the plain unbox stored the int as a callable descriptor.
pub(in crate::ir_lower) fn coerce_property_assign_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    php_type: &PhpType,
    span: Span,
) -> LoweredValue {
    let target = php_type.codegen_repr();
    if target == PhpType::Callable
        && ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Mixed
    {
        let effects = Op::mixed_unbox_effects(&target);
        return ctx.emit_owned_value(
            Op::MixedUnbox,
            vec![value.value],
            Some(Immediate::I64(10)),
            target,
            effects,
            Some(span),
        );
    }
    coerce_typed_assign_value(ctx, value, php_type, span)
}

/// Coerces a typed local assignment into the storage shape required by the declared type.
pub(in crate::ir_lower) fn coerce_typed_assign_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    php_type: &PhpType,
    span: Span,
) -> LoweredValue {
    let target_ty = php_type.codegen_repr();
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    let value = if source_ty == PhpType::Int && php_type.coerces_integer_to_float() {
        coerce_to_float(ctx, value, Some(span))
    } else {
        value
    };
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if source_ty == target_ty {
        return value;
    }
    let container_value = coerce_container_to_mixed_payload(
        ctx,
        value,
        &source_ty,
        &target_ty,
        Some(span),
    );
    if container_value.value != value.value {
        return container_value;
    }
    if crate::types::param_binding::object_requires_runtime_nominal_guard(php_type, &source_ty)
        || crate::types::param_binding::gradual_object_requires_runtime_nominal_guard(
            php_type,
            &source_ty,
        )
    {
        if let Some(target_name) =
            crate::types::param_binding::nominal_object_boundary_target(php_type)
        {
            let target_data = ctx.intern_class_name(&target_name);
            // The guard BORROWS its operand and hands back its OWN reference: with an object
            // source, `store_nominal_object_result` either boxes the payload into a fresh
            // Mixed cell or increfs it, because the guard's EIR result is declared owned.
            // An owning source temporary is dead from here on and has to be released, exactly
            // as `box_value_as_mixed` releases the producer it supersedes -- otherwise
            // `$h->p = new C()` into a `?C` slot leaves the object one reference above zero
            // forever: no `__destruct`, and the payload leaks.
            //
            // A GRADUAL source (Mixed / Union) is excluded on purpose. There the guard result
            // is the same boxed cell, marked borrowed by
            // `LoweringContext::value_is_borrowed_gradual_nominal_guard`, so the source must
            // outlive it.
            let release_source =
                matches!(source_ty, PhpType::Object(_)) && ctx.value_is_owning_temporary(value);
            let guarded = ctx.emit_value(
                Op::RuntimeCall,
                vec![value.value],
                Some(Immediate::NominalObject {
                    target: target_data,
                    boundary: crate::ir::NominalObjectBoundary::Property,
                }),
                PhpType::Object(target_name),
                effects_lookup::runtime_effects(),
                Some(span),
            );
            if release_source {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
            }
            return guarded;
        }
    }
    match target_ty {
        PhpType::Mixed => ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span)),
        // php's weak-mode float-to-int coercion at a declared `int` slot: an integral value
        // converts, a fractional one stores the truncated integer with the "loses precision"
        // deprecation, and NaN / infinity / out-of-range raise a `TypeError`.
        PhpType::Int if source_ty == PhpType::Float => ctx.emit_value(
            Op::FToI,
            vec![value.value],
            Some(Immediate::Bool(true)),
            PhpType::Int,
            Op::FToI.default_effects() | crate::ir::Effects::MAY_WARN,
            Some(span),
        ),
        PhpType::Str if matches!(source_ty, PhpType::Object(_)) => {
            coerce_to_string_at_span(ctx, value, Some(span))
        }
        target @ PhpType::AssocArray { .. } if matches!(source_ty, PhpType::Array(_)) => {
            // `ArrayToHash` consumes its input owner. An assignment from a borrowed local must
            // preserve that local's indexed owner while producing the independent hash stored in
            // the associative destination; a fresh expression already supplies the owned input.
            let source = if ctx.value_is_owning_temporary(value) {
                value
            } else {
                crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
            };
            ctx.emit_value(
                Op::ArrayToHash,
                vec![source.value],
                None,
                target,
                Op::ArrayToHash.default_effects(),
                Some(span),
            )
        }
        target @ (PhpType::Callable | PhpType::Object(_) | PhpType::Iterable)
            if source_ty == PhpType::Mixed =>
        {
            // The backend retains the extracted payload, so this value owns a lease of its own.
            // Marking it `MaybeOwned` made every consumer add a second `Acquire` on top, which is
            // one leaked object per `static::$prop = $mixed` store.
            let effects = Op::mixed_unbox_effects(&target);
            ctx.emit_owned_value(
                Op::MixedUnbox,
                vec![value.value],
                None,
                target,
                effects,
                Some(span),
            )
        }
        _ => value,
    }
}

/// Lowers a `foreach` loop using high-level iterator opcodes.
pub(super) fn lower_foreach(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    value_by_ref: bool,
    body: &[Stmt],
    loop_span: Span,
) {
    // A by-reference walk of a stdClass the checker checked as its property-name loop.
    if value_by_ref && crate::stdclass_by_ref_walk::is_site(loop_span) {
        if let Some(walk) =
            crate::stdclass_by_ref_walk::rewrite(array, key_var, value_var, body, loop_span)
        {
            super::lower_stmt(ctx, &walk);
            return;
        }
    }
    // Apply the checker-computed loop header contract before lowering the source expression so
    // an iterated-and-mutated array is loaded with its stable payload representation.
    let _ = apply_loop_storage_contracts(ctx, loop_span, Some(array.span));
    // A direct instance property has a dedicated fetch-for-write operation that separates and
    // republishes its container before iteration. Reifying it as a synthetic alias here bypasses
    // that operation and loses the property's copy-on-write boundary. Static properties and
    // nested elements still need an origin local so iterator relocation can follow replacements.
    let prepared_source = (value_by_ref
        && !matches!(array.kind, ExprKind::PropertyAccess { .. }))
    .then(|| prepare_addressable_by_ref_foreach_source(ctx, array))
    .flatten();
    let array = prepared_source.as_ref().unwrap_or(array);
    // Promote a by-reference indexed source BEFORE it is loaded, so the loop iterates the hash
    // the rest of the program will see in that local.
    let by_ref_origin = promote_by_ref_foreach_source(ctx, array, value_by_ref);
    let ForeachSource {
        value: mut source,
        is_borrowed_fetch: source_is_borrowed_fetch,
        receiver: mut source_receiver,
    } = lower_foreach_source(ctx, array, value_by_ref);
    if value_by_ref
        && by_ref_origin.is_none()
        && matches!(
            ctx.builder.value_php_type(source.value).codegen_repr(),
            PhpType::Array(_)
        )
    {
        let hash_ty = PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Mixed),
        };
        source = ctx.emit_value(
            Op::ArrayToHash,
            vec![source.value],
            None,
            hash_ty,
            Op::ArrayToHash.default_effects(),
            Some(array.span),
        );
    }
    // Orthogonal to the borrowed fetch-for-write pin taken after `IterStart` below: that one
    // keeps a by-reference element or property container alive, while this one takes the loop's
    // reference on an object source. Borrowed fetch-for-write sources are containers, never
    // objects, so `retain_object_foreach_source` returns them untouched and the flag still
    // describes `source`.
    let source = retain_object_foreach_source(ctx, source, array.span);
    // A by-value loop iterates a SNAPSHOT: `foreach ($key as $i => $s) { unset($key[$i]); }` still
    // visits every element. A boxed `mixed`/union local lends its array to the iterator without a
    // reference of its own, so the in-place `unset` shortened the very storage being walked and
    // the loop ended after one pass (Laravel's `data_get('tags.0')` returned the whole array). An
    // owned reference makes the write split instead, and the loop's exit releases it.
    let source = if !value_by_ref
        && !source_is_borrowed_fetch
        && matches!(array.kind, ExprKind::Variable(_))
        && matches!(
            ctx.builder.value_php_type(source.value).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
        && !ctx.value_is_owning_temporary(source)
    {
        crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, source, Some(array.span))
    } else {
        source
    };
    // By-value ordinary objects are normalized to their visible properties before `IterStart`,
    // and an `IteratorAggregate` (compiled or interpreted) to the iterator it hands out.
    let mut aggregate_root = None;
    let source = if value_by_ref || source_is_borrowed_fetch {
        source
    } else {
        let (source, walked_object) =
            normalize_plain_object_foreach_source(ctx, source, array.span);
        // A by-value source never reads through a receiver, so the pin slot is free for the
        // walked object, which `receiver_pin` then releases on every way out of the loop.
        if walked_object.is_some() {
            debug_assert!(source_receiver.is_none());
            source_receiver = walked_object;
        }
        let (iterator, root) = materialize_aggregate_iterator(ctx, source, array.span);
        aggregate_root = root;
        iterator
    };
    // Iterator initialization can invoke user code before the loop frame exists.
    // Publish temporary sources first so a same-frame catch can unwind them.
    // Every by-reference source is deliberately excluded because retaining it
    // before IterStart would defeat the required copy-on-write split. Concrete
    // array locals also need to remain a direct LoadLocal so the backend can
    // publish the split payload back into the original slot.
    let (source, source_owner) = if value_by_ref || source_is_borrowed_fetch {
        (source, None)
    } else {
        crate::ir_lower::expr::root_owned_call_operand(ctx, source, array.span)
    };
    let source_php_ty = ctx.builder.value_php_type(source.value);
    let source_ty = source_php_ty.codegen_repr();
    let key_needs_null_init = key_var.is_some_and(|name| !ctx.local_slots.contains_key(name));
    let value_needs_null_init = !ctx.local_slots.contains_key(value_var);
    // A foreach over a concretely-indexed array (`Array` of a non-Mixed element
    // type) always yields integer keys, even though `Op::IterCurrentKey` lowers
    // the key as Mixed. Tag the key local so a `$dst[$key] = ...` write coerces
    // the int-valued Mixed key to int instead of promoting the destination to a
    // hash. Generic `Array(Mixed)`, `AssocArray`, `Mixed`, and `Union` sources
    // may carry string keys and are left untagged so the write promotes.
    if let Some(key_var) = key_var {
        if let PhpType::Array(elem_ty) = &source_php_ty {
            if !matches!(elem_ty.as_ref(), PhpType::Mixed) {
                ctx.mark_foreach_int_key(key_var);
            }
        }
    }
    let (iterator, iterator_owner, iterator_state) =
        ctx.emit_iter_start_with_origin(source, value_by_ref, by_ref_origin, array.span);
    // php frees a temporary aggregate once `FE_RESET` has its iterator rewound; a `rewind()`
    // that throws releases the iterator first and the aggregate after it, which the unwind
    // order of the two owner records gives.
    if let Some(slot) = aggregate_root {
        retire_aggregate_root_under_iterator_owners(
            ctx,
            slot,
            &[source_owner, iterator_owner],
            array.span,
        );
    }
    // Take the loop's own lifetime reference on a borrowed fetch-for-write source after
    // `IterStart`.
    // The order is the whole point: `IterStart` splits a by-reference source through
    // `__rt_array_ensure_unique`, so a pin taken before it would put the element back at
    // refcount 2 and hand the loop a private copy — the very miscompile issue #580 fixes.
    // Taken here, the split has already happened and the iterator has already captured the
    // pointer, so the pin only keeps that storage alive.
    let source_pin = source_is_borrowed_fetch
        .then(|| pin_by_ref_foreach_borrowed_source(ctx, source, array.span))
        .flatten();
    // The container is BORROWED from a property slot the loop reached through a temporary --
    // `$arr[0]->x`, `$o->get()->x`, `$o->$n` on a temporary. The pin above keeps the container
    // alive, but the OBJECT that owns the slot is what the writes have to reach, so it is held
    // for the loop too and released on every way out (issue #690).
    let receiver_pin = source_receiver.map(|receiver| LoopCleanup {
        value: receiver,
        span: array.span,
    });
    if let Some(key_var) = key_var {
        initialize_foreach_mixed_local_if_needed(ctx, key_var, key_needs_null_init, array.span);
    }
    if value_by_ref {
        let value_ty = foreach_ref_value_type(&source_ty);
        // A name the body already bound keeps its earlier storage unless it is retyped here.
        // `$value = "held"; foreach ($items as &$value)` then read the boxed entry cell through
        // a `string` slot and echoed a pointer pair as a string.
        let value_slot = ctx.declare_local(value_var, value_ty.clone());
        ctx.set_local_type(value_var, value_ty.clone());
        if value_ty == PhpType::Mixed {
            if value_needs_null_init {
                // This setup runs once per OUTER iteration when the loop is nested, and from the
                // second one on the slot still holds the reference the previous run bound to an
                // element. The null initializer then wrote THROUGH it: `foreach ($a as &$inner)
                // { foreach ($inner as &$v) {...} }` nulled the last element of every inner array
                // but the final one (Symfony's `EventDispatcher::optimizeListeners()` lost its
                // listeners that way). php never assigns the value variable here, so detach it the
                // way `unset($v)` does before the initializer stores into the slot itself.
                for op in [Op::UnsetLocal, Op::ZeroLocalSlot] {
                    ctx.emit_void(
                        op,
                        Vec::new(),
                        Some(Immediate::LocalSlot(value_slot)),
                        op.default_effects(),
                        Some(array.span),
                    );
                }
            }
            initialize_foreach_mixed_local_if_needed(
                ctx,
                value_var,
                value_needs_null_init,
                array.span,
            );
        }
        if !value_needs_null_init {
            ctx.mark_local_initialized(value_var);
            if !ctx.is_ref_bound_local(value_var) {
                ctx.promote_local_ref_cell(value_var, Some(array.span));
            }
        }
    } else {
        let value_ty = foreach_value_type(&source_ty);
        if value_ty == PhpType::Mixed {
            initialize_foreach_mixed_local_if_needed(
                ctx,
                value_var,
                value_needs_null_init,
                array.span,
            );
        } else if value_needs_null_init {
            ctx.declare_local(value_var, value_ty.clone());
            ctx.set_local_type(value_var, value_ty);
        }
    }
    let header = ctx.builder.create_named_block("foreach.next", Vec::new());
    let body_block = ctx.builder.create_named_block("foreach.body", Vec::new());
    let exit = ctx.builder.create_named_block("foreach.exit", Vec::new());
    let first_nonempty = (value_by_ref
        && value_needs_null_init
        && foreach_ref_value_type(&source_ty) == PhpType::Mixed)
        .then(|| {
            ctx.builder
                .create_named_block("foreach.first_nonempty", Vec::new())
        });
    if let Some(first_nonempty) = first_nonempty {
        let has_next = ctx.emit_value(
            Op::IterNext,
            vec![iterator.value],
            None,
            PhpType::Bool,
            Op::IterNext.default_effects(),
            Some(array.span),
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: has_next.value,
            then_target: first_nonempty,
            then_args: Vec::new(),
            else_target: exit,
            else_args: Vec::new(),
        });

        ctx.builder.position_at_end(first_nonempty);
        let value_slot = ctx.declare_local(value_var, PhpType::Mixed);
        ctx.release_stored_local_value(value_var, value_slot, Some(array.span));
        branch_to(ctx, body_block);
    } else {
        branch_to(ctx, header);
    }

    ctx.builder.position_at_end(header);
    let has_next = ctx.emit_value(
        Op::IterNext,
        vec![iterator.value],
        None,
        PhpType::Bool,
        Op::IterNext.default_effects(),
        Some(array.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_next.value,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.clear_static_callable_locals();
    ctx.builder.position_at_end(body_block);
    // `IterStart` consumes a by-reference local load while making the source unique, then
    // publishes that replacement owner into the origin local. The SSA value no longer owns a
    // second reference after that transfer, so putting it in the loop cleanup would free the
    // live local table on normal and early exits. Sources without an origin, including literal
    // temporaries, keep their ordinary cleanup obligation.
    let cleanup = (source_owner.is_none()
        && by_ref_origin.is_none()
        && ctx.value_is_owning_temporary(source))
    .then_some(LoopCleanup {
        value: source,
        span: array.span,
    });
    let missing_strings = crate::ir_lower::expr::loop_region_missing_string_names(
        ctx, body, None, None,
    );
    ctx.push_loop_carried_missing_strings(missing_strings);
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: header,
        cleanup,
        source_owner: source_owner.map(|slot| (slot, array.span)),
        source_pin,
        iterator_owner: iterator_owner.map(|slot| (slot, array.span)),
        iterator_cleanup: by_ref_origin.map(|_| (iterator_state, array.span)),
        receiver_pin,
    });
    if let Some(key_var) = key_var {
        let key = ctx.emit_value(
            Op::IterCurrentKey,
            vec![iterator.value],
            None,
            PhpType::Mixed,
            Op::IterCurrentKey.default_effects(),
            Some(array.span),
        );
        ctx.store_local(key_var, key, PhpType::Mixed, Some(array.span));
    }
    if value_by_ref {
        let slot = ctx.declare_local(value_var, foreach_ref_value_type(&source_ty));
        ctx.release_ref_cell_owner(value_var, Some(array.span));
        ctx.emit_void(
            Op::IterCurrentValueRef,
            vec![iterator.value],
            Some(Immediate::LocalSlot(slot)),
            Op::IterCurrentValueRef.default_effects(),
            Some(array.span),
        );
        ctx.mark_ref_bound_local(value_var);
        ctx.mark_local_initialized(value_var);
    } else {
        let value_ty = foreach_value_type(&source_ty);
        let value = lower_foreach_current_value(ctx, iterator, &source_ty, &value_ty, array.span);
        ctx.store_local(value_var, value, value_ty, Some(array.span));
    }
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    ctx.pop_loop_carried_missing_strings();
    branch_to(ctx, header);
    ctx.builder.position_at_end(exit);
    // The exit is also reached by `break`, from states where the body had already STORED a value
    // that widened a local's slot to boxed storage; the types in force here are those of the
    // iterations that fell through to the next check. `$n = count($m); foreach ($m as $v) { if (…)
    // { $n = $m; break; } } echo json_encode($n);` read the boxed slot as `int`. The `while`
    // exit re-asserts the same widening.
    ctx.reassert_widened_local_storage_types();
    ctx.clear_static_callable_locals();
    if by_ref_origin.is_some() {
        ctx.emit_void(
            Op::IterEnd,
            Vec::new(),
            Some(Immediate::LocalSlot(iterator_state)),
            Op::IterEnd.default_effects(),
            Some(array.span),
        );
    }
    // Release the source when it is a fresh owning temporary (e.g. `foreach
    // (explode(...) as $p)` or a literal array): the iterator borrows it for the
    // duration of the loop, so nothing else frees it once iteration ends. (For an
    // array the iterator aliases the source, so it must NOT be released separately
    // — that would double-free.)
    if let Some(slot) = iterator_owner {
        ctx.retire_iter_start_owner(slot, array.span);
    }
    if let Some(slot) = source_owner {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, slot, array.span);
    } else if by_ref_origin.is_none() && ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(array.span));
    }
    // Normal termination is the exit this block IS, so the pin is dropped here. Every other way
    // out — `break`, `break N`, `return`, `throw` — skips this block and is covered by
    // `emit_innermost_loop_cleanups` through the loop frame instead.
    if let Some(pin) = source_pin {
        crate::ir_lower::ownership::release_if_owned(ctx, pin.value, Some(pin.span));
    }
    if let Some(pin) = receiver_pin {
        crate::ir_lower::ownership::release_if_owned(ctx, pin.value, Some(pin.span));
    }
}

/// Reifies a property, static property, or nested element source as a local reference.
///
/// Iterator relocation can only reload a local slot. A synthetic alias turns an addressable
/// non-local source into such a slot without copying the container: instance properties use
/// their promoted property cell, native static properties use their process-lifetime symbol
/// address, and nested elements first promote their parent to hash storage and then retain the
/// entry's tag-11 cell. Growth through any spelling writes into that same cell, so `IterNext`
/// always reloads the live table.
fn prepare_addressable_by_ref_foreach_source(
    ctx: &mut LoweringContext<'_, '_>,
    source: &Expr,
) -> Option<Expr> {
    match &source.kind {
        ExprKind::Variable(_) => Some(source.clone()),
        ExprKind::PropertyAccess { object, property }
            if crate::ir_lower::expr::by_ref_foreach_property_source_is_addressable(
                ctx, object, property,
            ) =>
        {
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            crate::ir_lower::expr::lower_ref_assign_property(ctx, &alias, source, source.span);
            Some(Expr::new(ExprKind::Variable(alias), source.span))
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            // The alias must be declared with the payload the static slot actually holds. An
            // untyped `public static $rows = [...]` keeps concrete hash storage, and declaring
            // the alias Mixed made the loop's write-back box that hash into a Mixed cell and
            // publish it through the symbol, so the next read walked a cell as a table.
            let value_type = crate::ir_lower::expr::static_property_result_type(
                ctx, receiver, property, source,
            );
            let alias = ctx.declare_synthetic_php_local(value_type);
            crate::ir_lower::expr::lower_ref_assign_static_property(
                ctx,
                &alias,
                source,
                source.span,
            );
            Some(Expr::new(ExprKind::Variable(alias), source.span))
        }
        ExprKind::ArrayAccess { array, index } => {
            let parent = prepare_addressable_by_ref_foreach_source(ctx, array)?;
            promote_by_ref_foreach_source(ctx, &parent, true)?;
            let element = Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(parent),
                    index: index.clone(),
                },
                source.span,
            );
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            crate::ir_lower::expr::lower_owned_ref_assign_array_elem(
                ctx,
                &alias,
                &element,
                source.span,
            );
            Some(Expr::new(ExprKind::Variable(alias), source.span))
        }
        _ => None,
    }
}

/// Prepares a simple array local as a relocatable by-reference `foreach` source.
///
/// `foreach ($arr as &$v)` makes every visited entry part of a PHP reference set, and elephc
/// records that membership in the entry's own persistent reference word. Only a hash entry has
/// one: an indexed array stores bare payload slots, so a by-reference loop over one bound the
/// alias straight to `payload + cursor * stride` and left nothing behind for a later reader to
/// find. `clone($object, $withProperties)`'s per-entry guard is exactly such a reader, so a live
/// alias into an indexed override array was silently accepted instead of raising php 8.5's
/// "Cannot assign by reference when cloning with updated properties".
///
/// Promotion reuses `Op::ArrayToHash` the way `unset($arr[$key])` already does: the indexed
/// payload becomes an integer-keyed hash (keys `0..n-1`, insertion order, heap children retained
/// and strings persisted) whose entries are then widened to boxed Mixed cells, which is where the
/// reference word lives. `Op::ArrayToHash` consumes the loaded owner and returns its replacement,
/// and `store_mutated_local` releases the local's own reference, so the array is freed exactly
/// once and the hash owns every child. The replacement local is typed `AssocArray<Int, Mixed>`;
/// earlier SSA values keep their indexed type, while every later operation selects hash lowering.
///
/// Only an indexed source needs physical promotion. An associative source is already a hash, but
/// it still has to return its local slot as the iterator origin: growth can replace that table just
/// as it can replace the promoted table. Element, instance-property, and static-property sources
/// are first represented by synthetic reference locals, so they use this same origin protocol.
///
/// Returns the promoted local slot so `IterStart` can record it as the iterator origin. Growth
/// and copy-on-write inside the loop body republish the replacement table into that slot, and
/// `IterNext` reloads it from there instead of walking the table captured at loop entry.
pub(crate) fn promote_by_ref_foreach_source(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    value_by_ref: bool,
) -> Option<LocalSlotId> {
    if !value_by_ref {
        return None;
    }
    let ExprKind::Variable(name) = &array.kind else {
        return None;
    };
    let slot = *ctx.local_slots.get(name.as_str())?;
    let source_ty = ctx.local_type(name).codegen_repr();
    if matches!(source_ty, PhpType::Mixed | PhpType::Iterable | PhpType::Union(_)) {
        return Some(slot);
    }
    if let PhpType::AssocArray { key, value } = source_ty {
        if value.codegen_repr() != PhpType::Mixed {
            let storage_ty = PhpType::AssocArray {
                key,
                value: Box::new(PhpType::Mixed),
            };
            // HashToMixed consumes its input owner at the COW boundary. A synthetic
            // reference alias loads the static/property/parent payload borrowed, so give
            // the conversion its own owner before it can retire the source generation.
            let hash = ctx.load_local(name, Some(array.span));
            let hash = if ctx.is_ref_bound_local(name) {
                crate::ir_lower::ownership::acquire_if_refcounted(
                    ctx,
                    hash,
                    Some(array.span),
                )
            } else {
                hash
            };
            let mixed = ctx.emit_value(
                Op::HashToMixed,
                vec![hash.value],
                None,
                storage_ty.clone(),
                Op::HashToMixed.default_effects(),
                Some(array.span),
            );
            ctx.store_mutated_local(name, mixed, storage_ty, Some(array.span));
        }
        return Some(slot);
    }
    let PhpType::Array(_) = source_ty else {
        return None;
    };
    let storage_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: Box::new(PhpType::Mixed),
    };
    // ArrayToHash consumes its input owner. Retain only a synthetic reference alias: an
    // ordinary local deliberately transfers its old owner into the conversion, while the
    // alias must leave the property or parent element's owner intact until store-back.
    let array_value = ctx.load_local(name, Some(array.span));
    let array_value = if ctx.is_ref_bound_local(name) {
        crate::ir_lower::ownership::acquire_if_refcounted(
            ctx,
            array_value,
            Some(array.span),
        )
    } else {
        array_value
    };
    // Both operands above are owners the conversion takes -- the local's transferred reference or
    // the alias's acquired one -- so the immediate tells codegen to consume it, which a slot load
    // cannot prove on its own. Without it `$a = [1]; $x = &$a[0];` leaked the packed array.
    let hash = ctx.emit_value(
        Op::ArrayToHash,
        vec![array_value.value],
        Some(crate::ir::Immediate::Bool(true)),
        storage_ty.clone(),
        Op::ArrayToHash.default_effects(),
        Some(array.span),
    );
    ctx.store_retyped_container_local(name, hash, storage_ty, Some(array.span));
    Some(slot)
}

/// Lowers the `foreach` source expression under the loop's binding mode.
///
/// A by-value loop iterates a copy, so it keeps the ordinary retaining read: the extra
/// reference is what makes `__rt_array_ensure_unique` copy, and copying is precisely the
/// semantics. A by-reference loop mutates the source in place, so an array-element or stable
/// object-property source is fetched for writing instead. Otherwise the read's own reference
/// makes the runtime copy the container and the loop writes into a discarded copy (issues #580
/// and #642).
///
/// Returns the lowered source together with whether it came back borrowed from the
/// fetch-for-write path, which is what tells the caller the loop still owes it a lifetime
/// reference of its own.
fn lower_foreach_source(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    value_by_ref: bool,
) -> ForeachSource {
    if value_by_ref {
        if let ExprKind::ArrayAccess {
            array: receiver,
            index,
        } = &array.kind
        {
            let source = lower_by_ref_foreach_element_source(ctx, receiver, index, array);
            let is_borrowed_element =
                ctx.builder.value_ownership(source.value) == Ownership::Borrowed;
            return ForeachSource {
                value: source,
                is_borrowed_fetch: is_borrowed_element,
                receiver: None,
            };
        }
        if let Some((object, property)) = by_ref_property_source_parts(array) {
            let source = lower_by_ref_foreach_property_source(ctx, object, &property, array);
            let is_borrowed_property =
                ctx.builder.value_ownership(source.value.value) == Ownership::Borrowed;
            return ForeachSource {
                value: source.value,
                is_borrowed_fetch: is_borrowed_property,
                receiver: source.receiver,
            };
        }
    }
    ForeachSource {
        value: lower_expr(ctx, array),
        is_borrowed_fetch: false,
        receiver: None,
    }
}

/// What `lower_foreach_source` hands back: the container, whether it came back borrowed from a
/// fetch-for-write read, and the receiver that read borrowed through.
struct ForeachSource {
    value: LoweredValue,
    is_borrowed_fetch: bool,
    receiver: Option<LoweredValue>,
}

/// Splits a by-reference `foreach` source into `(receiver, property)` when it reads a property.
///
/// A RUNTIME-named property is included when the name has already folded to a literal, which is
/// the ordinary case for `$o->$n` with a constant `$n`: the slot the backend resolves is the
/// same one the static spelling reaches, so there is no reason for the two spellings to differ
/// (issue #690). A name that is still an expression at this point keeps the ordinary read --
/// the backend's slot resolution needs the name.
fn by_ref_property_source_parts(array: &Expr) -> Option<(&Expr, String)> {
    match &array.kind {
        ExprKind::PropertyAccess { object, property } => Some((object, property.clone())),
        ExprKind::DynamicPropertyAccess { object, property } => match &property.kind {
            ExprKind::StringLiteral(name) => Some((object, name.clone())),
            _ => None,
        },
        _ => None,
    }
}

/// Takes the by-reference loop's own lifetime reference on a borrowed fetch-for-write source.
///
/// `ArrayGetForWrite` and `PropGetForWrite` return the container without a reference of their own:
/// the parent slot is the only owner. That makes writes land in the parent, but leaves the iterator
/// dangling if the loop body drops or replaces the parent. PHP avoids that by holding a reference
/// to the iterated array itself for the duration of the loop.
///
/// Must be called AFTER `Op::IterStart`: that instruction runs `__rt_array_ensure_unique` on a
/// by-reference source, and an extra reference held across it would make the split fire and give
/// the loop a private copy to write into.
///
/// Returns `None` when the source type carries no runtime lifetime state, in which case there is
/// nothing to pin and nothing to release.
fn pin_by_ref_foreach_borrowed_source(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    span: Span,
) -> Option<LoopCleanup> {
    let pin =
        crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, source, Some(span));
    if pin.value == source.value {
        return None;
    }
    // The acquire result is the loop's own reference, so pin it `Owned` explicitly instead of
    // leaving it at the `MaybeOwned` default: the cleanup paths below release through
    // `release_if_owned`, which the backend filters on this very state.
    ctx.builder.set_value_ownership(pin.value, Ownership::Owned);
    Some(LoopCleanup { value: pin, span })
}

/// Returns the by-value foreach local type when Phase 04 can keep a concrete element.
pub(super) fn foreach_value_type(source_ty: &PhpType) -> PhpType {
    match source_ty.codegen_repr() {
        PhpType::Array(elem) => match elem.codegen_repr() {
            PhpType::Callable => PhpType::Callable,
            PhpType::Object(class_name) => PhpType::Object(class_name),
            elem @ (PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool) => elem,
            // A pointer element is its raw word and may be the null pointer, so the binding is
            // `?ptr<T>` like an indexed read of the same element.
            pointer @ PhpType::Pointer(_) => crate::types::pointer_element_read_type(pointer),
            // A buffer handle is its raw word; a stored element is never null.
            buffer @ PhpType::Buffer(_) => buffer,
            _ => PhpType::Mixed,
        },
        // A hash of pointers binds the pointer word too (see `lower_foreach_current_value`); a
        // stored `null` is the word 0, so the binding keeps PHP null.
        PhpType::AssocArray { value, .. } if matches!(value.codegen_repr(), PhpType::Pointer(_)) => {
            crate::types::pointer_element_read_type(value.codegen_repr())
        }
        // A hash of buffers binds the handle word the same way.
        PhpType::AssocArray { value, .. } if matches!(value.codegen_repr(), PhpType::Buffer(_)) => {
            value.codegen_repr()
        }
        PhpType::Object(class_name) if class_name == "Phar" || class_name == "PharData" => {
            PhpType::Object("PharFileInfo".to_string())
        }
        _ => PhpType::Mixed,
    }
}

/// Emits the current foreach value with the type the loop variable binds.
///
/// Hash iteration hands every entry out boxed as `Mixed`, and a pointer or buffer entry is boxed
/// as an integer-like payload. For a hash of pointers or buffers the box is cast back to that
/// word and retyped, so `$p->x`, `$p === null` and `$buf[$i]` see the pointer or handle instead
/// of a boxed integer.
fn lower_foreach_current_value(
    ctx: &mut LoweringContext<'_, '_>,
    iterator: LoweredValue,
    source_ty: &PhpType,
    value_ty: &PhpType,
    span: Span,
) -> LoweredValue {
    let unbox_hash_word = matches!(source_ty, PhpType::AssocArray { .. })
        && matches!(value_ty.codegen_repr(), PhpType::Pointer(_) | PhpType::Buffer(_));
    let current_ty = if unbox_hash_word { PhpType::Mixed } else { value_ty.clone() };
    let value = ctx.emit_value(
        Op::IterCurrentValue,
        vec![iterator.value],
        None,
        current_ty,
        Op::IterCurrentValue.default_effects(),
        Some(span),
    );
    if !unbox_hash_word {
        return value;
    }
    let word = crate::ir_lower::expr::coerce_to_int_at_span(ctx, value, Some(span));
    ctx.emit_value(
        Op::Move,
        vec![word.value],
        None,
        value_ty.clone(),
        Op::Move.default_effects(),
        Some(span),
    )
}

/// Returns the local value type used when a foreach binds the value by reference.
pub(super) fn foreach_ref_value_type(source_ty: &PhpType) -> PhpType {
    match source_ty.codegen_repr() {
        // By-reference iteration widens indexed slots to boxed Mixed storage before binding.
        // The local must use that same shape so reads, writes, and final alias cleanup operate
        // on the box stored in the element rather than interpreting its pointer as a scalar.
        PhpType::Array(_) => PhpType::Mixed,
        // Hash foreach references point at boxed Mixed entry slots. Keeping the local Mixed
        // lets assignment replace the entry cell without interpreting its pointer as a typed
        // scalar payload, and preserves PHP's ability to change the referenced value's type.
        PhpType::AssocArray { .. } => PhpType::Mixed,
        _ => PhpType::Mixed,
    }
}

/// Initializes a fresh foreach loop variable to boxed null before the first iteration.
pub(super) fn initialize_foreach_mixed_local_if_needed(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    needs_init: bool,
    span: Span,
) {
    if !needs_init {
        return;
    }
    // This setup can run once per outer-loop iteration at runtime, overwriting
    // the loop variable. `store_local` owns the carried release: it frees the
    // previous runtime occupant when this synthetic store is loop-carried.
    ctx.declare_local(name, PhpType::Mixed);
    ctx.set_local_type(name, PhpType::Mixed);
    let null = emit_null_value(ctx, Some(span));
    let boxed = ctx.box_value_as_mixed(null, PhpType::Mixed, Some(span));
    ctx.store_foreach_initializer_local_only(name, boxed, PhpType::Mixed, Some(span));
}

/// Takes the loop's own reference on an object `foreach` source.
///
/// Iterating an object — a user `Iterator`/`IteratorAggregate`, or a `Generator` —
/// must keep it alive for the whole loop even when the body drops every other
/// owner (`foreach ($it as $v) { unset($it); }`), so the loop needs a reference of
/// its own. `Op::IterStart` used to take that reference with a bare backend
/// `incref` that nothing ever balanced, leaking the object and everything it owned
/// once per loop. It is taken here instead, as an `Op::Acquire` whose result is an
/// owning temporary: the loop's exit block and its `LoopCleanup` (early `return`,
/// multi-level `break`) already release such a value exactly once.
///
/// The reference the *lowered source expression* carried is dropped right away
/// under the pre-existing "owning temporary" rule, so a fresh
/// `foreach (make_iter() as $v)` temporary is still released exactly once — just
/// before the loop rather than after it, which the acquire above makes safe.
///
/// Non-object sources are returned untouched: the iterator aliases an array or
/// hash source, so retaining one would change its refcount and therefore its
/// copy-on-write behaviour inside the loop body.
fn retain_object_foreach_source(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    span: Span,
) -> LoweredValue {
    if !matches!(
        ctx.builder.value_php_type(source.value).codegen_repr(),
        PhpType::Object(_)
    ) {
        return source;
    }
    let retained = crate::ir_lower::ownership::acquire_if_refcounted(ctx, source, Some(span));
    if retained.value == source.value {
        return source;
    }
    if ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
    retained
}

/// Iterates an `IteratorAggregate` through the iterator its `getIterator()` returns.
///
/// The backend used to call `getIterator()` inside `IterStart` and keep the result in the iterator
/// state, where no exit path ever released it: every `foreach` over an aggregate leaked the
/// iterator object and everything it held (an `ArrayIterator` with its array, 7-9 blocks), and
/// Laravel collections are aggregates. Asking here makes the iterator an owning temporary that
/// the loop's ordinary source pin and exit release already balance.
///
/// A temporary aggregate is rooted in an unwind-visible owner slot, returned with the iterator, and
/// retired by the caller right after `IterStart`: php's `FE_RESET` frees its operand once the
/// iterator exists and is rewound, a `getIterator()` that throws still destroys it on the way to
/// the catch, and a `rewind()` that throws destroys the iterator before it.
fn materialize_aggregate_iterator(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<crate::ir::LocalSlotId>) {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(source.value).codegen_repr() else {
        return (source, None);
    };
    let implements = |ctx: &LoweringContext<'_, '_>, interface: &str| {
        crate::ir_lower::expr::array_access_types::class_implements_interface_for_ir(
            ctx,
            &class_name,
            interface,
        )
    };
    // An `IteratorAggregate`-typed parameter names the interface itself, which no lookup reports
    // as implementing itself.
    let is_aggregate = class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("IteratorAggregate")
        || implements(ctx, "IteratorAggregate");
    if !is_aggregate || implements(ctx, "Iterator") {
        return (source, None);
    }
    if !ctx.value_is_owning_temporary(source) {
        let iterator = crate::ir_lower::expr::lower_zero_arg_method_call(ctx, source, "getIterator", span);
        return (iterator, None);
    }
    let (_, root) = crate::ir_lower::expr::root_owned_call_operand(ctx, source, span);
    let Some(root) = root else {
        // Nothing to root: the call releases an owning receiver temporary itself.
        let iterator = crate::ir_lower::expr::lower_zero_arg_method_call(ctx, source, "getIterator", span);
        return (iterator, None);
    };
    // The call reads the aggregate through a borrowed load of its root, so the owner slot keeps
    // the only reference and the call does not release it.
    let root_name = ctx
        .local_slots
        .iter()
        .find_map(|(name, slot)| (*slot == root).then(|| name.clone()))
        .expect("a rooted operand names its hidden owner slot");
    let receiver = ctx.load_local(&root_name, Some(span));
    let iterator = crate::ir_lower::expr::lower_zero_arg_method_call(ctx, receiver, "getIterator", span);
    (iterator, Some(root))
}

/// Releases a temporary aggregate's owner record while the iterator's records stay published.
///
/// Call-operand owner records form a stack and `PopCallOperandOwner` drops the innermost one, but
/// the aggregate was rooted BEFORE the iterator it produced. The records pushed after it
/// (`above`, innermost last) are unlinked without a release, the aggregate is retired, and they
/// are pushed back in their original order, so the loop's own exits still find them.
fn retire_aggregate_root_under_iterator_owners(
    ctx: &mut LoweringContext<'_, '_>,
    aggregate_root: crate::ir::LocalSlotId,
    above: &[Option<crate::ir::LocalSlotId>],
    span: Span,
) {
    let above: Vec<_> = above.iter().flatten().copied().collect();
    for slot in above.iter().rev() {
        ctx.emit_void(
            Op::PopCallOperandOwner,
            Vec::new(),
            Some(Immediate::LocalSlot(*slot)),
            Op::PopCallOperandOwner.default_effects(),
            Some(span),
        );
    }
    crate::ir_lower::expr::retire_owned_call_operand(ctx, aggregate_root, span);
    for slot in above {
        ctx.emit_void(
            Op::PushCallOperandOwner,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            Op::PushCallOperandOwner.default_effects(),
            Some(span),
        );
    }
}

/// Casts an ordinary concrete object to its visible property hash before `foreach` iteration.
///
/// Returns the projection and, when the object was an owning temporary, the object itself: php's
/// loop holds the object it walks, so `foreach (new Probe() as ...)` destroys it when the loop
/// lets go, not before the first iteration. The caller pins it for every exit of the loop.
fn normalize_plain_object_foreach_source(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<LoweredValue>) {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(source.value).codegen_repr() else {
        return (source, None);
    };
    // A parameter typed with one of the traversal interfaces names the interface itself, which no
    // lookup reports as implementing itself: `function sum(IteratorAggregate $a)` walked an empty
    // property projection and summed nothing.
    let traversable = ["Traversable", "Iterator", "IteratorAggregate"]
        .iter()
        .any(|name| class_name.trim_start_matches('\\').eq_ignore_ascii_case(name))
        // An interface that inherits `Traversable` (Symfony's `ConstraintViolationListInterface`)
        // leaves the question to the run-time object: walking its properties loses every element.
        || crate::ir_lower::expr::array_access_types::interface_extends_interface_for_ir(
            ctx,
            class_name.trim_start_matches('\\'),
            "Traversable",
        )
        || crate::ir_lower::expr::array_access_types::class_implements_interface_for_ir(
            ctx,
            &class_name,
            "Iterator",
        )
        || crate::ir_lower::expr::array_access_types::class_implements_interface_for_ir(
            ctx,
            &class_name,
            "IteratorAggregate",
        );
    if traversable {
        return (source, None);
    }
    // The projection is scope-filtered by the function's lexical class, so it serves a walk from
    // any scope: outside the class it holds only the public properties, as php visits them.
    let result_type = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };
    let result = ctx.emit_value(
        Op::Cast,
        vec![source.value],
        Some(Immediate::CastTarget(IrType::Heap(crate::ir::IrHeapKind::Hash))),
        result_type,
        Op::Cast.default_effects(),
        Some(span),
    );
    let kept = ctx.value_is_owning_temporary(source).then_some(source);
    (result, kept)
}
