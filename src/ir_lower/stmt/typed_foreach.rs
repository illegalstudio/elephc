//! Purpose:
//! Typed assignments and foreach iterator lowering.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

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
    let lowered = coerce_typed_assign_value(ctx, lowered, &php_type, span);
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

/// Coerces a typed local assignment into the storage shape required by the declared type.
pub(super) fn coerce_typed_assign_value(
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
    match target_ty {
        PhpType::Mixed => ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span)),
        target @ (PhpType::Callable | PhpType::Object(_)) if source_ty == PhpType::Mixed => {
            ctx.emit_value(
                Op::MixedUnbox,
                vec![value.value],
                None,
                target,
                Op::MixedUnbox.default_effects(),
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
    // Apply the checker-computed loop header contract before lowering the source expression so
    // an iterated-and-mutated array is loaded with its stable payload representation.
    apply_loop_storage_contracts(ctx, loop_span, Some(array.span));
    let (source, source_is_borrowed_fetch) = lower_foreach_source(ctx, array, value_by_ref);
    // Orthogonal to the borrowed fetch-for-write pin taken after `IterStart` below: that one
    // keeps a by-reference element or property container alive, while this one takes the loop's
    // reference on an object source. Borrowed fetch-for-write sources are containers, never
    // objects, so `retain_object_foreach_source` returns them untouched and the flag still
    // describes `source`.
    let source = retain_object_foreach_source(ctx, source, array.span);
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
    let iterator = ctx.emit_value(
        Op::IterStart,
        vec![source.value],
        value_by_ref.then_some(Immediate::Bool(true)),
        PhpType::Iterable,
        Op::IterStart.default_effects(),
        Some(array.span),
    );
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
    if let Some(key_var) = key_var {
        initialize_foreach_mixed_local_if_needed(ctx, key_var, key_needs_null_init, array.span);
    }
    if value_by_ref {
        let value_ty = foreach_ref_value_type(&source_ty);
        ctx.declare_local(value_var, value_ty.clone());
        ctx.set_local_type(value_var, value_ty);
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
    branch_to(ctx, header);

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
    let cleanup = ctx
        .value_is_owning_temporary(source)
        .then_some(LoopCleanup {
            value: source,
            span: array.span,
        });
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: header,
        cleanup,
        source_pin,
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
        let value = ctx.emit_value(
            Op::IterCurrentValue,
            vec![iterator.value],
            None,
            value_ty.clone(),
            Op::IterCurrentValue.default_effects(),
            Some(array.span),
        );
        ctx.store_local(value_var, value, value_ty, Some(array.span));
    }
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    branch_to(ctx, header);
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
    // Release the source when it is a fresh owning temporary (e.g. `foreach
    // (explode(...) as $p)` or a literal array): the iterator borrows it for the
    // duration of the loop, so nothing else frees it once iteration ends. (For an
    // array the iterator aliases the source, so it must NOT be released separately
    // — that would double-free.)
    if ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(array.span));
    }
    // Normal termination is the exit this block IS, so the pin is dropped here. Every other way
    // out — `break`, `break N`, `return`, `throw` — skips this block and is covered by
    // `emit_innermost_loop_cleanups` through the loop frame instead.
    if let Some(pin) = source_pin {
        crate::ir_lower::ownership::release_if_owned(ctx, pin.value, Some(pin.span));
    }
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
) -> (LoweredValue, bool) {
    if value_by_ref {
        if let ExprKind::ArrayAccess {
            array: receiver,
            index,
        } = &array.kind
        {
            let source = lower_by_ref_foreach_element_source(ctx, receiver, index, array);
            let is_borrowed_element =
                ctx.builder.value_ownership(source.value) == Ownership::Borrowed;
            return (source, is_borrowed_element);
        }
        if let ExprKind::PropertyAccess { object, property } = &array.kind {
            let source = lower_by_ref_foreach_property_source(ctx, object, property, array);
            let is_borrowed_property =
                ctx.builder.value_ownership(source.value) == Ownership::Borrowed;
            return (source, is_borrowed_property);
        }
    }
    (lower_expr(ctx, array), false)
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
    // A Resource element must be recognized BEFORE `codegen_repr()`, which collapses it
    // to Int: the scalar arm below then returned that Int and
    // `foreach ([STDIN, STDOUT, STDERR] as $s) { stream_get_meta_data($s); }` was
    // refused with "stream argument PHP type Int".
    if let PhpType::Array(elem) = source_ty {
        if matches!(**elem, PhpType::Resource(_)) {
            return (**elem).clone();
        }
    }
    match source_ty.codegen_repr() {
        PhpType::Array(elem) => match elem.codegen_repr() {
            PhpType::Callable => PhpType::Callable,
            PhpType::Object(class_name) => PhpType::Object(class_name),
            elem @ (PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool) => elem,
            _ => PhpType::Mixed,
        },
        PhpType::Object(class_name) if class_name == "Phar" || class_name == "PharData" => {
            PhpType::Object("PharFileInfo".to_string())
        }
        _ => PhpType::Mixed,
    }
}

/// Returns the local value type used when a foreach binds the value by reference.
pub(super) fn foreach_ref_value_type(source_ty: &PhpType) -> PhpType {
    match source_ty.codegen_repr() {
        // A Resource element must survive inference: collapsing it to its codegen
        // repr turned `foreach ($handles as $h)` into an untyped int and lost every
        // registry check downstream.
        PhpType::Array(elem) => *elem,
        PhpType::AssocArray { value, .. } => *value,
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
