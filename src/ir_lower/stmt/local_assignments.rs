//! Purpose:
//! Local assignment, storage contextualization, and reference assignment.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers a plain PHP local assignment.
pub(super) fn lower_assign(ctx: &mut LoweringContext<'_, '_>, name: &str, value: &Expr, span: Span) {
    // PHP allows compound assignment on an undefined variable (`$x += 1`),
    // treating the undefined variable as null/0 with a warning. The type
    // checker injects the variable as `Void` and emits a warning. At the
    // lowering level, we must initialize the local slot to null/0 before
    // the compound read so the runtime does not read garbage from the stack.
    if is_compound_assignment_self_read(value, name, span) && !ctx.has_local_slot(name) {
        let null_value = ctx.builder.emit_const_null();
        let null_lowered = LoweredValue { value: null_value, ir_type: IrType::I64 };
        ctx.store_local(name, null_lowered, PhpType::Void, Some(span));
        ctx.mark_local_initialized(name);
    }
    initialize_undefined_direct_assignment_source(ctx, value);

    // A by-reference `Closure::bind(fn &() => $this->prop, $obj, $obj)` assigned to a variable is
    // tracked as a static callable, like a closure literal, so a later `$b()` lowers to a direct
    // call that carries the property's reference-cell pointer instead of boxing it.
    let bound_closure = crate::ir_lower::expr::is_bound_closure_assignment_shape(ctx, value);
    let direct_closure = matches!(value.kind, ExprKind::Closure { .. }) || bound_closure;
    ctx.clear_pending_static_callable_result();
    let static_callable = static_callable_binding_for_expr(ctx, value);
    let reflected_class = reflection_class_binding_for_expr(ctx, value);
    let reflected_function = reflection_function_binding_for_expr(ctx, value);
    let reflected_property = reflection_property_binding_for_expr(ctx, value);
    let reflected_method = reflection_method_binding_for_expr(ctx, value);
    let reflected_args = reflection_arg_array_binding_for_expr(value);
    let fiber_start_sig = crate::ir_lower::fibers::start_sig_for_expr(ctx, value);
    let callable_array = lower_callable_array_for_assignment(ctx, value, static_callable.as_ref());
    let lowered = callable_array
        .as_ref()
        .map(|assignment| assignment.value)
        .or_else(|| lower_closure_for_assignment(ctx, name, value))
        .or_else(|| {
            bound_closure
                .then(|| crate::ir_lower::expr::lower_bound_closure_for_assignment(ctx, value))
                .flatten()
        })
        .unwrap_or_else(|| lower_expr(ctx, value));
    // The checker recorded this assignment as an incompatible RE-BINDING of `$name`: end the old
    // binding here, so the store below mints a fresh slot at the new type instead of widening a
    // slot the program will never read at the old type again.
    //
    // Placed exactly here for two reasons. The right-hand side is already lowered, so
    // `$a = "n=" . $a` (and every compound assignment, which the parser hands over in this same
    // shape) still reads the OLD binding. And it is before `contextualize_local_assignment`,
    // whose whole job is to coerce the value into the storage contract the name ALREADY has —
    // the contract of the binding being abandoned, which must not apply to the new one.
    if ctx.is_recorded_retype_site(span, name) {
        ctx.rebind_local_for_retype(name, Some(span));
    }
    // The checker's pre-scan marked `$name` as assigned incompatible types across a branch, so its
    // frame storage is boxed `Mixed` for the WHOLE body (`checker::mixed_storage_scan`), and this
    // is one of the assignments it recorded. Two things follow, and BOTH are needed.
    //
    // The slot is declared `Mixed` here, ahead of the store. Every write to a marked name is a
    // recorded store site — anything else disqualifies the name — so this runs at the FIRST one,
    // and `has_local_slot` makes it idempotent for the rest. (A name the frame already bound keeps
    // that slot: the declare is skipped, not re-typed. The one marked name that can arrive already
    // bound in the body's incoming environment is a by-value closure capture, and the pre-scan
    // MARKS those rather than refusing them — dropping the mark strands the value the capture owns.
    // Whether the mark also WARNS is decided by replaying the name's assignments from the capture's
    // INCOMING type, the replay `--strict-locals` itself would run: a rejection makes the warning's
    // advice true and pushes it, a clean merge would make that advice false and the mark stays
    // silent. Both ways the store sites reach here, and what carries the boxed contract for such a
    // name is the forced `Mixed` STORE type below: it re-types the local from the first recorded
    // store onwards, so every read after that one is a boxed read, while a read BEFORE it still
    // sees the incoming value at the type it actually arrived with.) Letting the first store mint
    // a slot at its own value's type is what made a marked program miscompile:
    // `$a = 123456789; for (…) { $a = "s"; }` wrote a raw int into an `Int` slot that the loop
    // body then widened, and a zero-trip loop read the surviving int back through the widened
    // string view (`string(9) "123456789"`).
    //
    // And the store's PHP type is `Mixed`, not the value's own. `store_local` records it as the
    // local's LOGICAL type, and every read of the name is lowered against that one: inside the
    // arm that stored it, inside the copies DCE's tail-sinking makes of the code BELOW the branch,
    // and after the join (`join_arm_types` keeps nothing for an `int`-against-`string`
    // disagreement, so the merge inherits the last arm's fact). Leaving those facts concrete is
    // what turned `if (…) { $a = 42; } else { $a = "hello"; } echo strlen($a);` into a compiler
    // PANIC — "strlen cannot lower checked operand type Int" — from the copy of the `echo` sunk
    // into the `int` arm. `Mixed` on every store makes every read a boxed read, which is exactly
    // the type the checker bound for the name. This mirrors `boxed_incdec_storage_type`, the other
    // whole-frame boxed-storage contract, which forces the same substitution inside `store_local`.
    //
    // That is also what makes the checker's flow NARROWING harmless here. Inside
    // `if (is_string($a)) { strlen($a); }` the checker types this marked name `Str` while its slot
    // is boxed — but lowering never sees that fact. `load_local` types every read from the
    // LOWERING's own `local_types`, which this store just set to `Mixed`, and nothing on this side
    // narrows on a type guard (statement-level conditionals snapshot and restore `local_types`
    // across arms; they never refine them). So the load is emitted at the slot's real storage type
    // and the guard's narrowing turns into an unbox/cast APPLIED TO THE LOADED VALUE, inside the
    // branch, where the runtime tag check belongs. The narrowing is a diagnostics decision — it
    // says whether `strlen($a)` type-checks — never a load-shape one.
    //
    // Nothing is forced for a name backed by program-global storage: `store_local` overrides the
    // type with `global_alias_type` (already `Mixed`) and stores through the global symbol, so a
    // marked top-level local another body writes via `global $a` keeps exactly the representation
    // it has today.
    let mixed_storage_site = ctx.is_recorded_mixed_storage_site(span, name);
    if mixed_storage_site && !ctx.has_local_slot(name) {
        ctx.declare_local(name, PhpType::Mixed);
    }
    let (lowered, php_type) = contextualize_local_assignment(ctx, name, value, lowered, span);
    let php_type = if mixed_storage_site {
        PhpType::Mixed
    } else {
        php_type
    };
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
    if !closure_captures_local(value, name) {
        if let Some(target) = static_callable {
            ctx.bind_static_callable_local(name, target);
        }
    }
    if let Some(reflected_class) = reflected_class {
        ctx.bind_reflection_class_local(name, reflected_class);
    }
    if let Some(reflected_function) = reflected_function {
        ctx.bind_reflection_function_local(name, reflected_function);
    }
    if let Some((reflected_class, reflected_property)) = reflected_property {
        ctx.bind_reflection_property_local(name, reflected_class, reflected_property);
    }
    if let Some((reflected_class, reflected_method)) = reflected_method {
        ctx.bind_reflection_method_local(name, reflected_class, reflected_method);
    }
    if let Some(reflected_args) = reflected_args {
        ctx.bind_reflection_arg_array_local(name, reflected_args);
    }
    if let Some(sig) = fiber_start_sig {
        ctx.bind_fiber_start_sig(name, sig);
    }
}

/// Materializes PHP null and a path-sensitive warning when a direct assignment reads an
/// otherwise undefined local, so unreachable loop bodies remain harmless and reachable reads
/// match php-src instead of loading an uninitialized frame slot.
fn initialize_undefined_direct_assignment_source(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
) {
    let ExprKind::Variable(source) = &value.kind else {
        return;
    };
    if ctx.has_local_slot(source) || ctx.variable_has_runtime_initializer(source) {
        return;
    }
    let warning = Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::unqualified("__elephc_diag_warning"),
            args: vec![
                Expr::new(
                    ExprKind::StringLiteral(format!("\nWarning: Undefined variable ${}", source)),
                    value.span,
                ),
                Expr::new(ExprKind::IntLiteral(value.span.line as i64), value.span),
                Expr::new(ExprKind::IntLiteral(2), value.span),
            ],
        },
        value.span,
    );
    let warning_value = lower_expr(ctx, &warning);
    release_expr_statement_result(ctx, warning_value, value.span);
    let null_value = ctx.builder.emit_const_null();
    let null_lowered = LoweredValue {
        value: null_value,
        ir_type: IrType::I64,
    };
    ctx.store_local(source, null_lowered, PhpType::Void, Some(value.span));
    ctx.mark_local_initialized(source);
}

/// Returns whether a closure literal captures the local being assigned.
pub(super) fn closure_captures_local(value: &Expr, name: &str) -> bool {
    matches!(
        &value.kind,
        ExprKind::Closure { captures, capture_refs, .. }
            if captures.iter().any(|capture| capture == name)
                || capture_refs.iter().any(|capture| capture == name)
    )
}

/// Coerces an assignment into the stable storage contract already known for its local.
///
/// Array payload promotion applies to every RHS shape, while indexed-to-associative conversion
/// remains limited to literals whose ordered entries can be materialized as hash pairs.
pub(super) fn contextualize_local_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    value: &Expr,
    lowered: LoweredValue,
    span: Span,
) -> (LoweredValue, PhpType) {
    let source_ty = ctx.builder.value_php_type(lowered.value);
    let source_repr = source_ty.codegen_repr();
    let contextual_ty = if crate::superglobals::is_superglobal(name) {
        crate::superglobals::superglobal_type()
    } else {
        ctx.local_type(name)
    };
    let contextual_repr = contextual_ty.codegen_repr();
    let has_loop_contract = local_has_loop_storage_contract(ctx, name, &contextual_ty);

    // A first assignment precedes the loop whose contract was computed from the final checker
    // environment. Preserve the literal's natural storage here; the loop preheader owns the
    // one-time conversion, including empty arrays whose runtime header still needs promotion.
    if has_loop_contract && !ctx.has_local_slot(name) {
        return (lowered, source_ty);
    }

    if matches!(contextual_repr, PhpType::Mixed) && has_loop_contract {
        let converted = if matches!(source_repr, PhpType::Mixed | PhpType::Union(_)) {
            lowered
        } else {
            ctx.box_value_as_mixed(lowered, contextual_ty.clone(), Some(span))
        };
        return (converted, contextual_ty);
    }

    if matches!(
        (&source_repr, &contextual_repr),
        (
            PhpType::Array(source_element),
            PhpType::Array(target_element)
        ) if source_element.codegen_repr() != PhpType::Mixed
            && target_element.codegen_repr() == PhpType::Mixed
    ) {
        let converted = coerce_container_to_mixed_payload(
            ctx,
            lowered,
            &source_repr,
            &contextual_repr,
            span,
        );
        return (converted, contextual_ty);
    }

    if matches!(
        (&source_repr, &contextual_repr),
        (
            PhpType::AssocArray {
                value: source_value,
                ..
            },
            PhpType::AssocArray {
                value: target_value,
                ..
            }
        ) if source_value.codegen_repr() != PhpType::Mixed
            && target_value.codegen_repr() == PhpType::Mixed
    ) {
        let converted = coerce_container_to_mixed_payload(
            ctx,
            lowered,
            &source_repr,
            &contextual_repr,
            span,
        );
        return (converted, contextual_ty);
    }

    if matches!(value.kind, ExprKind::ArrayLiteral(_))
        && matches!(source_repr, PhpType::Array(_))
        && matches!(contextual_repr, PhpType::AssocArray { .. })
    {
        let hash = ctx.emit_value(
            Op::ArrayToHash,
            vec![lowered.value],
            None,
            contextual_ty.clone(),
            Op::ArrayToHash.default_effects(),
            Some(span),
        );
        return (hash, contextual_ty);
    }

    // No storage contract applies: keep the value's own checker type. `codegen_repr()` is lossy
    // (`Resource(_)` -> `Int`, `Union(_)` -> `Mixed`/`TaggedScalar`, `False` -> `Bool`), and the
    // local's recorded type feeds `is_resource`/`gettype` and nullable-union lowering, so the
    // collapsed form must stay confined to the storage-contract comparisons above.
    (lowered, source_ty)
}

/// Returns whether this function-like scope records `target` for `name` on any loop header.
pub(super) fn local_has_loop_storage_contract(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    target: &PhpType,
) -> bool {
    ctx.loop_storage_types.iter().any(|((scope, _), contracts)| {
        scope == &ctx.loop_storage_scope
            && contracts
                .get(name)
                .is_some_and(|contract| contract.codegen_repr() == target.codegen_repr())
    })
}

/// Lowers a by-reference assignment, dispatching on the kind of reference source.
///
/// - `$a = &$b` aliases two locals to one ref-cell.
/// - `$a = &$obj->prop` binds the local to the object's reference-property cell (write-through).
/// - `$a = &call()` binds the local to the cell returned by a by-reference callee.
/// - `$a = &$arr[idx]` binds the local to the indexed-array element's inline storage.
pub(super) fn lower_ref_assign(ctx: &mut LoweringContext<'_, '_>, target: &str, source: &Expr, span: Span) {
    match &source.kind {
        ExprKind::Variable(source_name) => {
            let fiber_start_sig = ctx.fiber_start_sig_for_local(source_name);
            ctx.alias_local_ref_cell(target, source_name, Some(span));
            if let Some(sig) = fiber_start_sig {
                ctx.bind_fiber_start_sig(target, sig);
            }
        }
        ExprKind::PropertyAccess { .. } => {
            crate::ir_lower::expr::lower_ref_assign_property(ctx, target, source, span);
        }
        ExprKind::FunctionCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. } => {
            crate::ir_lower::expr::lower_ref_assign_call(ctx, target, source, span);
        }
        ExprKind::ArrayAccess { .. } => {
            crate::ir_lower::expr::lower_ref_assign_array_elem(ctx, target, source, span);
        }
        _ => {
            // Other source shapes are rejected by the checker;
            // evaluate for side effects to keep lowering total.
            lower_expr(ctx, source);
        }
    }
}
