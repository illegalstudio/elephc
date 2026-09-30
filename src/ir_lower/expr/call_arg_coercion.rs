//! Purpose:
//! General call argument lowering and parameter storage coercion.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers positional/named/spread call arguments in source order.
pub(super) fn lower_args(ctx: &mut LoweringContext<'_, '_>, args: &[Expr]) -> Vec<crate::ir::ValueId> {
    args.iter()
        .enumerate()
        .map(|(index, arg)| {
            let value = lower_expr(ctx, arg);
            if index + 1 < args.len() {
                root_evaluated_call_argument(ctx, value, arg.span).value
            } else {
                value.value
            }
        })
        .collect()
}

/// Lowers one argument while applying by-reference storage normalization from a signature.
pub(super) fn lower_arg_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> crate::ir::ValueId {
    lower_arg_with_signature_for(ctx, sig, index, arg, None)
}

/// [`lower_arg_with_signature`] knowing the callee php would NAME in a TypeError.
///
/// Only the user-call sites can supply it, and only they need it: a builtin's own argument
/// refusals are composed elsewhere.
pub(super) fn lower_arg_with_signature_for(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
    callee: Option<&str>,
) -> crate::ir::ValueId {
    if sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(
            &arg.kind,
            ExprKind::ArrayAccess { array, .. }
                if !matches!(&array.kind, ExprKind::Variable(_))
        )
    {
        if let Some((alias, aliases)) = prepare_scoped_addressable_ref_array_receiver(ctx, arg) {
            let value = lower_arg_with_signature_for(ctx, sig, index, &alias, callee);
            retire_scoped_ref_receiver_aliases(ctx, &aliases);
            return value;
        }
    }
    if let Some(value) = lower_by_ref_array_element_arg_with_signature(ctx, sig, index, arg) {
        return value;
    }
    if let Some(value) = lower_by_ref_array_arg_with_signature(ctx, sig, index, arg) {
        return value;
    }
    guard_boxed_object_reference_argument(ctx, sig, index, arg);
    promote_boxed_reference_local_argument(ctx, sig, index, arg);
    promote_reference_return_local_argument(ctx, sig, index, arg);
    if let Some(lowered) = lower_tracked_callable_array_param(ctx, sig, index, arg) {
        return lowered.value;
    }
    let lowered = lower_expr(ctx, arg);
    coerce_scalar_arg_to_param_storage(ctx, sig, index, lowered, arg, callee).value
}

/// Creates the variables a call's BY-REFERENCE parameters are about to bind.
///
/// Binding a name by reference creates it in PHP rather than reading it, so no diagnostic is
/// raised and the variable is NULL afterwards: `function f(&$x) { $x = 7; } f($nope);` prints
/// `int(7)` in silence — MEASURED on `php -n` 8.5.6, against the by-VALUE spelling of the same
/// call, which warns. Without the store the argument reached the backend as an
/// `warned_null`, which has no by-reference form, and the program was refused.
///
/// It lives in `lower_args_with_signature` so every call shape gets it from one place: plain
/// functions, instance and static methods, nullable method calls and closure calls all lower
/// their arguments through there. Placing it at the function-call site only left the method,
/// static-method and closure spellings of the same program warning twice and answering NULL.
///
/// This is only the CREATION. `prepare_by_ref_null_out_locals` keeps its own narrower rule
/// about converting the slot to a Mixed cell, which answers a different question — what the
/// callee will WRITE — and deliberately does not run for builtins.
pub(super) fn create_by_ref_arg_locals(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) {
    let regular = crate::types::call_args::regular_param_count(sig);
    let by_ref_variadic = super::variadic_args::variadic_param_is_by_ref(sig);
    for (index, arg) in args.iter().enumerate() {
        let binds_by_ref = if index >= regular {
            by_ref_variadic
        } else {
            sig.ref_params.get(index).copied().unwrap_or(false)
        };
        if !binds_by_ref {
            continue;
        }
        let ExprKind::Variable(name) = &arg.kind else {
            continue;
        };
        let declared = sig.params.get(index).map(|(_, ty)| ty.clone());
        create_by_ref_arg_local(ctx, name, declared.as_ref(), arg, None);
    }
}

/// Creates ONE by-reference argument's variable, in storage the callee can write through.
///
/// The value is usually PHP's null, because that is what the variable holds until the callee
/// writes. The STORAGE follows the parameter's declared type: a `mixed` out-parameter — which
/// is how every builtin with an out-parameter declares one, `stream_socket_server`'s
/// `$error_code` and `$error_message` included — needs a boxed cell, and creating a bare null
/// slot for it made the backend refuse with `by-ref string output written into a null slot`,
/// taking `examples/udp-socket` and `examples/udg-socket` from wrong output to no output. An
/// untyped user parameter keeps the plain null slot, which is what its caller reads back.
///
/// `filled` overrides that null, because SOME callees do not write a value into the caller's
/// slot at all — they FILL the array the caller handed over, in place. `preg_match` is the one
/// such builtin: MEASURED, `preg_match("/(a)?(b)/", "b", $matches)` on an undeclared `$matches`
/// answered `bool(true)` where php answers the three-element array, because the boxed null cell
/// was written through as though it were an array. The type is not decided here — it is the one
/// shared answer `by_ref_fill` gives the checker too.
pub(super) fn create_by_ref_arg_local(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    declared: Option<&PhpType>,
    arg: &Expr,
    filled: Option<PhpType>,
) {
    // A name `global` bound has no frame slot and is not undefined: the global symbol holds it.
    if !ctx.local_name_is_undefined(name) || ctx.local_uses_global_storage(name) {
        return;
    }
    if let Some(filled) = filled {
        let empty = Expr::new(ExprKind::ArrayLiteral(Vec::new()), arg.span);
        let lowered = lower_expr(ctx, &empty);
        ctx.set_local_type(name, filled.clone());
        ctx.store_local(name, lowered, filled, Some(arg.span));
        return;
    }
    if matches!(declared, Some(PhpType::Mixed)) {
        let null = lower_boxed_null(ctx, arg);
        ctx.set_local_type(name, PhpType::Mixed);
        ctx.store_local(name, null, PhpType::Mixed, Some(arg.span));
        return;
    }
    let null = lower_null(ctx, arg);
    ctx.store_local(name, null, PhpType::Void, Some(arg.span));
}

/// Converts every local holding NULL that a USER function's by-reference parameter WRITES.
///
/// This is php's out-parameter idiom — `$x = null; f($x);` with `function f(&$a) { $a = 5; }` —
/// and the whole point of it is that `$x` is `int(5)` afterwards. The checker widened the
/// parameter to `mixed` when it saw the body write, and re-typed its own view of the caller's
/// variable; the LOWERING keeps its own local-type map, so without this the load after the call
/// still carried `php=null` and every read of it constant-folded to NULL. The write itself
/// always happened — the callee stores through the pointer — which is why nothing warned.
///
/// It runs from the USER-function call site only, and deliberately NOT from the shared argument
/// lowering that builtins also reach. A builtin's by-reference `mixed` parameter carries its own
/// convention for what the caller hands over: `stream_select($r, $w, $e, 0)` passes `null` for
/// the write and except sets, which the runtime reads as EMPTY sets, so boxing them into Mixed
/// cells made it read a cell header as an array length — it polled fourteen uninitialized
/// entries and answered 15 where php answers 1.
pub(super) fn prepare_by_ref_null_out_locals(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) {
    let Some(sig) = sig else {
        return;
    };
    // The name has to EXIST before the load below, or that load is an undefined read and what
    // gets boxed into the Mixed cell is the warning's null rather than the caller's slot.
    create_by_ref_arg_locals(ctx, sig, args);
    // A by-reference VARIADIC needs the same treatment past its fixed parameters: every argument
    // in the tail binds to `&...$out`, and `$out[$i] = …` writes through to the caller's cell. A
    // caller local still holding `null` has no Mixed storage for that write to land in, so
    // `foreach ($out as $i => $_) { $out[$i] = …; }` over `$a = null` left every variable NULL.
    let regular = crate::types::call_args::regular_param_count(sig);
    let by_ref_variadic = super::variadic_args::variadic_param_is_by_ref(sig);
    for (index, arg) in args.iter().enumerate() {
        let in_variadic_tail = index >= regular;
        if in_variadic_tail {
            if !by_ref_variadic {
                continue;
            }
        } else {
            if !sig.ref_params.get(index).copied().unwrap_or(false) {
                continue;
            }
            let Some((_, param_ty)) = sig.params.get(index) else {
                continue;
            };
            if !matches!(param_ty, PhpType::Mixed) {
                continue;
            }
        }
        let ExprKind::Variable(name) = &arg.kind else {
            continue;
        };
        // A name `global` bound lives in the global symbol, not in a frame slot: it has no local
        // type to read, and it is NOT undefined — nulling it here would erase the global.
        if ctx.local_uses_global_storage(name) {
            continue;
        }
        if !matches!(ctx.local_type(name), PhpType::Void) {
            continue;
        }
        let local = ctx.load_local(name, Some(arg.span));
        // The slot really is converted, not merely re-typed: the callee writes a boxed value
        // through the pointer, so what the caller hands over has to already be a Mixed cell.
        // This mirrors the `ArrayToMixed` the by-reference array path emits for the same reason.
        let boxed = ctx.emit_value(
            Op::MixedBox,
            vec![local.value],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(arg.span),
        );
        ctx.store_call_normalized_local(name, boxed, PhpType::Mixed, Some(arg.span));
    }
}

/// Captures a by-value runtime-parser argument while preserving by-reference places.
pub(super) fn lower_arg_with_signature_options(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
    capture_values: bool,
) -> crate::ir::ValueId {
    lower_arg_with_signature_options_for(ctx, sig, index, arg, capture_values, None)
}

/// [`lower_arg_with_signature_options`] knowing the callee php would NAME in a TypeError.
fn lower_arg_with_signature_options_for(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
    capture_values: bool,
    callee: Option<&str>,
) -> crate::ir::ValueId {
    if !capture_values {
        return lower_arg_with_signature_for(ctx, sig, index, arg, callee);
    }
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        promote_captured_reference_argument(ctx, arg);
        return lower_arg_with_signature_for(ctx, sig, index, arg, callee);
    }
    let lowered = lower_expr(ctx, arg);
    capture_call_argument_value(ctx, lowered, index, arg.span).value
}

/// Promotes a runtime-parser output local at its source-order argument evaluation point.
/// The managed reference retains storage identity without capturing the previous PHP value.
pub(super) fn promote_captured_reference_argument(ctx: &mut LoweringContext<'_, '_>, arg: &Expr) {
    match &arg.kind {
        ExprKind::Variable(name) => {
            let was_ref_bound = ctx.is_ref_bound_local(name);
            ctx.promote_local_mixed_ref_cell(name, Some(arg.span));
            if was_ref_bound {
                // A previous branch can mark the local without promoting every incoming path.
                ctx.promote_local_ref_cell(name, Some(arg.span));
            }
        },
        ExprKind::NamedArg { value, .. } => promote_captured_reference_argument(ctx, value),
        _ => {},
    }
}

/// Detaches a mutable boxed cell or retains a heap payload until its by-value call consumes it.
pub(super) fn capture_call_argument_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    parameter: usize,
    span: Span,
) -> LoweredValue {
    let ty = ctx.builder.value_php_type(value.value);
    let captured = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        let captured = ctx.emit_owned_value(Op::MixedClone, vec![value.value], None, ty,
            Op::MixedClone.default_effects(), Some(span));
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        captured
    } else {
        let captured = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, value, Some(span));
        if captured.value != value.value && ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        captured
    };
    ctx.guard_call_argument(captured, parameter, span);
    captured
}

/// Rechecks a declared object reference when an earlier call has changed its boxed payload.
fn guard_boxed_object_reference_argument(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return;
    }
    let declared_object = match sig.params.get(index).map(|(_, ty)| ty) {
        Some(PhpType::Object(class_name)) => Some(class_name.as_str()),
        Some(PhpType::Mixed) => match sig.param_type_exprs.get(index).and_then(Option::as_ref) {
            Some(TypeExpr::Named(name)) if !name.as_str().eq_ignore_ascii_case("mixed") => {
                Some(name.as_str())
            }
            _ => None,
        },
        _ => None,
    };
    let Some(class_name) = declared_object else {
        return;
    };
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };
    if ctx.local_type(name).codegen_repr() != PhpType::Mixed {
        return;
    }
    let condition = if class_name.is_empty() || class_name.eq_ignore_ascii_case("object") {
        let value = lower_expr(ctx, arg);
        let condition = ctx.emit_value(
            Op::TypePredicate,
            vec![value.value],
            Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)),
            PhpType::Bool,
            Op::TypePredicate.default_effects(),
            Some(arg.span),
        );
        if ctx.value_needs_release_after_use(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(arg.span));
        }
        condition
    } else {
        lower_instanceof(
            ctx,
            arg,
            &InstanceOfTarget::Name(Name::from(class_name)),
            arg,
        )
    };
    let accepted = ctx.builder.create_named_block("object.ref.accepted", Vec::new());
    let rejected = ctx.builder.create_named_block("object.ref.rejected", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value,
        then_target: accepted,
        then_args: Vec::new(),
        else_target: rejected,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(rejected);
    let message = Expr::new(
        ExprKind::StringLiteral(format!("Argument must be of type {}", class_name)),
        arg.span,
    );
    let exception = lower_expr(ctx, &Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("TypeError"),
            args: vec![message],
        },
        arg.span,
    ));
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
    ctx.builder.position_at_end(accepted);
}

/// Gives an eligible whole-local reference the canonical boxed payload required by the callee.
///
/// The checker permits this only for an eligible whole local. An existing object alias already
/// shares a boxed cell, so promotion never relabels a concrete cell behind another alias.
fn promote_boxed_reference_local_argument(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) {
    if !sig.ref_params.get(index).copied().unwrap_or(false)
        || !sig
            .params
            .get(index)
            .is_some_and(|(_, ty)| matches!(ty, PhpType::Mixed | PhpType::Object(_)))
    {
        return;
    }
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };
    if !ctx.boxed_reference_promotion_is_authorized(name, arg.span) {
        return;
    }
    if !ctx.is_ref_bound_local(name) && ctx.local_is_promotable_to_ref_cell(name) {
        ctx.promote_local_mixed_ref_cell(name, Some(arg.span));
    }
}

/// Materializes a proven callable array as the descriptor required by a Callable slot.
///
/// A tracked PHP local remains an ordinary array, and its instance receiver was already captured
/// into a hidden local at assignment. A literal crossing this boundary is consumed directly, so
/// its receiver is evaluated exactly once while constructing the descriptor.
pub(super) fn lower_tracked_callable_array_param(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<LoweredValue> {
    if sig.ref_params.get(index).copied().unwrap_or(false)
        || sig.params.get(index)?.1.codegen_repr() != PhpType::Callable
    {
        return None;
    }
    let target = match static_callable_binding_for_expr(ctx, arg)? {
        StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        StaticCallableBinding::InstanceMethod {
            object,
            method,
            direct_call: true,
            ..
        } => {
            CallableTarget::Method { object, method }
        }
        StaticCallableBinding::UserFunction(_)
        | StaticCallableBinding::ExternFunction(_)
        | StaticCallableBinding::Builtin(_)
        | StaticCallableBinding::Closure { .. }
        | StaticCallableBinding::StaticMethod { .. }
        | StaticCallableBinding::InstanceMethod {
            direct_call: false,
            ..
        } => return None,
    };
    Some(lower_first_class_callable(ctx, &target, arg))
}

/// Gives an escaping by-reference return a managed owner for a caller local.
///
/// A reference-returning callee can hand a by-reference parameter's address back to the caller.
/// A raw caller frame address has no `__rt_reference_cell_owner`, so promote the local before the
/// call and pass a fresh `LoadRefCell` marker that resolves to the managed cell pointer. Array
/// normalization runs first so the cell stores the final parameter-compatible representation.
fn promote_reference_return_local_argument(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) {
    if !sig.by_ref_return || !sig.ref_params.get(index).copied().unwrap_or(false) {
        return;
    }
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };
    ctx.promote_local_ref_cell(name, Some(arg.span));
}

/// Protects an incidental value view of an earlier reference place without replacing the place.
///
/// A local whose final frame storage widens to `Mixed` can make its narrower `LoadLocal<Str>`
/// allocate an owned detached string during codegen. The call still needs that exact load as its
/// by-reference place marker, so retain it as the operand and let the evaluation ledger treat the
/// protected value view as an intermediate owner. An exception from a later argument then retires
/// the detached view, while successful call materialization still recovers the original local slot.
fn root_prior_argument_preserving_reference_place(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
    value: crate::ir::ValueId,
) -> crate::ir::ValueId {
    let lowered = lowered_value_from_id(ctx, value);
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        if matches!(arg.kind, ExprKind::Variable(_))
            && sig.params.get(index).is_some_and(|(_, ty)| ty.codegen_repr() == PhpType::Str)
            && ctx.builder.value_php_type(value).codegen_repr() == PhpType::Str
        {
            let _protected_view = root_evaluated_call_argument(ctx, lowered, arg.span);
        }
        return value;
    }
    root_evaluated_call_argument(ctx, lowered, arg.span).value
}

/// Coerces a positional argument to storage owned explicitly by EIR when required.
///
/// Integer-to-float conversion selects the callee's floating-point ABI class. Mixed-to-string
/// conversion is also explicit here because it allocates caller-owned storage whose lifetime
/// depends on the call's return/argument alias contract; leaving that conversion hidden in ABI
/// materialization would give EIR no value to transfer or release after the call.
pub(super) fn coerce_scalar_arg_to_param_storage(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    arg: &Expr,
    callee: Option<&str>,
) -> LoweredValue {
    let Some((_, param_ty)) = sig.params.get(index) else {
        return value;
    };
    // A by-reference parameter must receive the caller's storage, not a converted temporary,
    // so declared-parameter scalar binding never applies to one. The checker keeps those on
    // the strict path for the same reason.
    let bindable = sig.declared_params.get(index).copied().unwrap_or(false)
        && !sig.ref_params.get(index).copied().unwrap_or(false);
    let param_ty = param_ty.codegen_repr();
    if value.ir_type == IrType::I64 && param_ty == PhpType::Float {
        return coerce_to_float(ctx, value, arg);
    }
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if param_ty == PhpType::Callable
        && !sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
    {
        return unbox_callable_param_storage(ctx, value, Some(arg.span));
    }
    if param_ty == PhpType::Str && matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
        return coerce_to_string(ctx, value, arg);
    }
    if bindable
        && matches!(param_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        && matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
    {
        return coerce_mixed_to_array_param(ctx, sig, index, value, arg, callee);
    }
    if bindable {
        if let Some(cast) = crate::types::param_binding::scalar_param_cast(&param_ty, &source_ty) {
            return apply_scalar_param_cast(ctx, cast, value, Some(arg.span));
        }
    }
    value
}

/// Unboxes a `Mixed` argument into a declared `array` parameter, throwing php's TypeError if it
/// holds anything else.
///
/// TWO TYPE MAPS disagreed here, in silence. A foreach over a nested literal gives the loop
/// variable the INNER array's inferred type while its STORAGE stays a boxed Mixed, so the checker
/// was happy and the call handed the callee the BOX where it expected the array. MEASURED on
/// `php -n` 8.5.6:
///
/// ```text
/// function takesArray(array $f): string { return implode("|", $f); }
/// foreach ([["plain", "two"]] as $r) { echo takesArray($r); }
/// php:    plain|two
/// elephc: |||
/// ```
///
/// The callee read the box's own header as an array of FOUR empty strings, one of them
/// `string(33794)`. The same value reaching `SplFileObject::fputcsv()` SEGFAULTED `__rt_fputcsv`
/// on a null element pointer, while the plain `fputcsv($h, $r, …)` builtin took it correctly —
/// which is what proved the value was fine and the BINDING was not.
///
/// `__rt_expect_array_arg` is the same helper the `array|false` builtin arguments use: it unboxes
/// tag 4 (indexed) and tag 5 (assoc) and throws with the caller's message otherwise. The result is
/// BORROWED — the unboxed pointer is the box's own storage, so an owned release would free the
/// array under the box.
fn coerce_mixed_to_array_param(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    arg: &Expr,
    callee: Option<&str>,
) -> LoweredValue {
    let Some(callee) = callee else {
        return value;
    };
    let Some((param, param_ty)) = sig.params.get(index) else {
        return value;
    };
    let param_ty = param_ty.clone();
    // php's own wording, MEASURED: `takesArray(): Argument #1 ($fields) must be of type array,
    // string given, called in FILE on line N`. The location tail is the throw site's, which
    // `__rt_expect_array_arg` appends from the instruction's own span.
    let message = format!(
        "{}(): Argument #{} (${}) must be of type array, given value is not an array",
        callee,
        index + 1,
        param
    );
    let data = ctx.intern_string(&message);
    let message = ctx
        .builder
        .emit_with_effects(
            Op::ConstStr,
            Vec::new(),
            Some(Immediate::Data(data)),
            IrType::Str,
            PhpType::Str,
            Ownership::Persistent,
            Op::ConstStr.default_effects(),
            Some(arg.span),
        )
        .expect("const_str produces a value");
    let member_ir = crate::ir_lower::context::return_ir_type(&param_ty);
    let unboxed = ctx
        .builder
        .emit_with_effects(
            Op::RuntimeCall,
            vec![value.value, message],
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
                crate::builtins::array_or_false::EXPECT_ARRAY_ARG,
            ))),
            member_ir,
            param_ty,
            Ownership::Borrowed,
            Op::RuntimeCall.default_effects(),
            Some(arg.span),
        )
        .expect("expect_array_arg produces a value");
    LoweredValue {
        value: unboxed,
        ir_type: member_ir,
    }
}

/// Extracts a statically checked callable whose merge storage became a Mixed cell.
/// The backend retains the descriptor, so the extracted EIR value must own that lease.
pub(super) fn unbox_callable_param_storage(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let result = ctx.emit_owned_value(
        Op::MixedUnbox,
        vec![value.value],
        None,
        PhpType::Callable,
        Op::mixed_unbox_effects(&PhpType::Callable),
        span,
    );
    release_coerced_source_if_owned(ctx, value, span);
    result
}

/// Applies a declared-parameter scalar binding to an already-lowered argument value.
///
/// The conversion is the one elephc emits for the equivalent explicit cast, which is why the
/// binding is expressed as a `CastType`: `(string)` and `(bool)` are total over the scalar
/// sources `crate::types::param_binding` admits, so no runtime failure path is needed here.
fn apply_scalar_param_cast(
    ctx: &mut LoweringContext<'_, '_>,
    cast: CastType,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    match cast {
        CastType::String => coerce_to_string_at_span(ctx, value, span),
        CastType::Bool => lower_truthy_bool(ctx, value, span),
        // `param_binding::scalar_param_cast` only ever reports the two total scalar casts.
        CastType::Int | CastType::Float | CastType::Array | CastType::Object => value,
    }
}

/// Normalizes reordered call operands to their declared scalar parameter storage.
///
/// Named and spread arguments are evaluated in source order and then reordered, so their
/// int-to-float and Mixed-to-string conversions happen here in parameter order. By-reference
/// parameters and the variadic tail remain untouched. String conversions become owned EIR
/// values so normal alias-aware call cleanup can transfer or release them safely.
pub(super) fn coerce_operands_to_params(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    mut operands: Vec<crate::ir::ValueId>,
) -> Vec<crate::ir::ValueId> {
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let limit = operands.len().min(regular_param_count);
    let needs_coercion = (0..limit)
        .map(|index| operand_needs_param_coercion(ctx, sig, &operands, index))
        .collect::<Vec<_>>();
    if needs_coercion.iter().any(|needs_coercion| *needs_coercion) {
        // Source evaluation has completed, but any parameter conversion below can throw.
        // Publish the one last source value that did not need protection during source
        // evaluation, plus synthesized owned operands, before the first conversion runs.
        for index in 0..limit {
            if sig.ref_params.get(index).copied().unwrap_or(false) {
                continue;
            }
            let lowered = lowered_value_from_id(ctx, operands[index]);
            operands[index] =
                root_evaluated_call_argument(ctx, lowered, Span::dummy()).value;
        }
    }
    for index in 0..limit {
        if sig.ref_params.get(index).copied().unwrap_or(false) {
            continue;
        }
        let Some((_, param_ty)) = sig.params.get(index) else {
            continue;
        };
        let value = operands[index];
        let operand_ty = ctx.builder.value_php_type(value).codegen_repr();
        let param_ty = param_ty.codegen_repr();
        if param_ty == PhpType::Float && matches!(operand_ty, PhpType::Int | PhpType::Bool) {
            let lowered = LoweredValue {
                value,
                ir_type: IrType::I64,
            };
            let coerced = coerce_to_float_at_span(ctx, lowered, None);
            operands[index] =
                root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
        } else if param_ty == PhpType::Str
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_))
        {
            let lowered = LoweredValue {
                value,
                ir_type: ctx.builder.value_type(value),
            };
            let coerced = coerce_to_string_at_span(ctx, lowered, None);
            operands[index] =
                root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
        } else if param_ty == PhpType::Callable
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_))
        {
            let lowered = LoweredValue { value, ir_type: ctx.builder.value_type(value) };
            let coerced = unbox_callable_param_storage(ctx, lowered, None);
            operands[index] =
                root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
        } else if sig.declared_params.get(index).copied().unwrap_or(false) {
            // Same declared-parameter scalar binding the positional path applies, run here in
            // parameter order because named and spread arguments are lowered in source order
            // and only reordered afterwards.
            if let Some(cast) =
                crate::types::param_binding::scalar_param_cast(&param_ty, &operand_ty)
            {
                let lowered = LoweredValue {
                    value,
                    ir_type: ctx.builder.value_type(value),
                };
                let coerced = apply_scalar_param_cast(ctx, cast, lowered, None);
                operands[index] =
                    root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
            }
        }
    }
    operands
}

/// Returns whether parameter-order normalization will emit a conversion for one operand.
fn operand_needs_param_coercion(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    operands: &[crate::ir::ValueId],
    index: usize,
) -> bool {
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        return false;
    }
    let Some((_, param_ty)) = sig.params.get(index) else {
        return false;
    };
    let operand_ty = ctx.builder.value_php_type(operands[index]).codegen_repr();
    let param_ty = param_ty.codegen_repr();
    (param_ty == PhpType::Float && matches!(operand_ty, PhpType::Int | PhpType::Bool))
        || (param_ty == PhpType::Str
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_)))
        || (param_ty == PhpType::Callable
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_)))
        || (sig.declared_params.get(index).copied().unwrap_or(false)
            && crate::types::param_binding::scalar_param_cast(&param_ty, &operand_ty).is_some())
}

/// Normalizes concrete local arrays to the storage required by their by-reference parameter.
pub(super) fn lower_by_ref_array_arg_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<crate::ir::ValueId> {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    let (_, param_ty) = sig.params.get(index)?;
    let ExprKind::Variable(name) = &arg.kind else {
        return None;
    };
    if param_ty.is_php_array()
        && matches!(ctx.local_type(name).codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. })
    {
        let local = ctx.load_local(name, Some(arg.span));
        let boxed = ctx.box_value_as_mixed(local, param_ty.clone(), Some(arg.span));
        ctx.store_call_argument_local(name, boxed, param_ty.clone(), Some(arg.span));
        promote_reference_return_local_argument(ctx, sig, index, arg);
        return Some(ctx.load_local(name, Some(arg.span)).value);
    }
    let (op, array_ty) = by_ref_array_arg_storage_conversion(ctx, name, param_ty)?;
    let local = ctx.load_local(name, Some(arg.span));
    // No op for an EMPTY array: `array<never>` has no element slots to box, so the caller only
    // needs its LOCAL re-typed to what the callee will fill it with. Emitting `ArrayToMixed`
    // there would tag the storage `mixed` when the callee is about to write raw ints into it.
    let normalized = match op {
        Some(op) => ctx.emit_value(
            op,
            vec![local.value],
            None,
            array_ty.clone(),
            op.default_effects(),
            Some(arg.span),
        ),
        None => local,
    };
    ctx.store_call_normalized_local(name, normalized, array_ty, Some(arg.span));
    promote_reference_return_local_argument(ctx, sig, index, arg);
    Some(ctx.load_local(name, Some(arg.span)).value)
}

/// Lowers `$array[$index]` as a direct by-reference argument cell address.
pub(super) fn lower_by_ref_array_element_arg_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<crate::ir::ValueId> {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    let ExprKind::ArrayAccess { array, index: element_index } = &arg.kind else {
        return None;
    };
    let ExprKind::Variable(array_name) = &array.kind else {
        return None;
    };
    let local_ty = ctx.local_type(array_name).codegen_repr();
    let (_, param_ty) = sig.params.get(index)?;
    if let PhpType::AssocArray { .. } = local_ty {
        let hash_value = ctx.load_local(array_name, Some(array.span));
        let element_index = lower_expr(ctx, element_index);
        let cell = ctx.emit_value(
            Op::LoadArrayElemRefCell,
            vec![hash_value.value, element_index.value],
            None,
            PhpType::Pointer(None),
            Op::LoadArrayElemRefCell.default_effects(),
            Some(arg.span),
        );
        return Some(lease_managed_call_argument_ref_cell(ctx, cell, arg.span).value);
    }
    if matches!(local_ty, PhpType::Mixed | PhpType::Union(_)) {
        // A declared PHP array reads back as one boxed value, so neither concrete branch above
        // matches even though the element is a managed cell just the same. Leasing it here is
        // what puts the cell in the call's unwind ledger: without that, a later argument whose
        // evaluation throws left the cell unreleased.
        let array_value = ctx.load_local(array_name, Some(array.span));
        let element_index = lower_expr(ctx, element_index);
        let cell = ctx.emit_value(
            Op::LoadArrayElemRefCell,
            vec![array_value.value, element_index.value],
            None,
            PhpType::Pointer(None),
            Op::LoadArrayElemRefCell.default_effects(),
            Some(arg.span),
        );
        return Some(lease_managed_call_argument_ref_cell(ctx, cell, arg.span).value);
    }
    let PhpType::Array(elem_ty) = local_ty else {
        return None;
    };
    if elem_ty.codegen_repr() == PhpType::Mixed {
        let array_value = ctx.load_local(array_name, Some(array.span));
        let element_index = lower_expr(ctx, element_index);
        let cell = ctx.emit_value(
            Op::LoadArrayElemRefCell,
            vec![array_value.value, element_index.value],
            None,
            PhpType::Pointer(None),
            Op::LoadArrayElemRefCell.default_effects(),
            Some(arg.span),
        );
        return Some(lease_managed_call_argument_ref_cell(ctx, cell, arg.span).value);
    }
    if param_ty.codegen_repr() == PhpType::Mixed && elem_ty.codegen_repr() != PhpType::Mixed {
        // The callee replaces a Mixed pointer through this element's actual slot, not a
        // detached temporary. Widen the outer array's slots before exposing that address.
        // Retaining the borrowed source lets the consuming conversion separate COW aliases.
        let parent = ctx.load_local(array_name, Some(array.span));
        let owned = crate::ir_lower::ownership::acquire_if_refcounted(ctx, parent, Some(arg.span));
        let boxed_parent_ty = PhpType::Array(Box::new(PhpType::Mixed));
        let converted = ctx.emit_value(
            Op::ArrayToMixed,
            vec![owned.value],
            None,
            boxed_parent_ty.clone(),
            Op::ArrayToMixed.default_effects(),
            Some(arg.span),
        );
        ctx.store_call_argument_local(
            array_name, converted, boxed_parent_ty, Some(arg.span),
        );
        // The parent now stores boxed slots, so this element is a managed cell like any other
        // `array<mixed>` element. Handing out a bare interior address instead left the FIRST of
        // two aliases of one element pointing at a slot the second argument then replaced with a
        // reference cell, so the callee read that cell pointer as the element's value.
        let array_value = ctx.load_local(array_name, Some(array.span));
        let element_index = lower_expr(ctx, element_index);
        let cell = ctx.emit_value(
            Op::LoadArrayElemRefCell,
            vec![array_value.value, element_index.value],
            None,
            PhpType::Pointer(None),
            Op::LoadArrayElemRefCell.default_effects(),
            Some(arg.span),
        );
        return Some(lease_managed_call_argument_ref_cell(ctx, cell, arg.span).value);
    }
    let array_value = ctx.load_local(array_name, Some(array.span));
    let element_index = lower_expr(ctx, element_index);
    let element_index = coerce_to_int_at_span(ctx, element_index, Some(arg.span));
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ArrayElemAddr,
            vec![array_value.value, element_index.value],
            None,
            IrType::I64,
            PhpType::Pointer(None),
            Ownership::NonHeap,
            Op::ArrayElemAddr.default_effects(),
            Some(arg.span),
        )
        .expect("array_elem_addr produces a value");
    Some(value)
}

/// Returns the conversion, if any, that puts a by-reference argument's storage in the element
/// representation its callee was compiled for, and the type the caller's local then carries.
///
/// Three cases, all measured against `php -n` 8.5.6:
/// - a LIST whose elements must become boxed needs `Op::ArrayToMixed`;
/// - a HASH needs `Op::HashToMixed`. Leaving the hash out meant `f($assoc)` with
///   `function f(array &$a)` handed a raw-slot hash to a body compiled for boxed ones, and
///   `["x" => 1, "y" => 2]` came back as two ADDRESSES;
/// - an EMPTY array — `array<never>` — needs NO op at all. It has no element slots, so the
///   caller only has to READ it as whatever the callee fills it with. `$e = []; fill($e);`
///   answered an empty array (and segfaulted before the by-reference widening landed) because
///   the caller kept reading `array<never>` storage the callee had already appended to.
fn by_ref_array_arg_storage_conversion(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    param_ty: &PhpType,
) -> Option<(Option<Op>, PhpType)> {
    let local_ty = ctx.local_type(name).codegen_repr();
    match (param_ty.codegen_repr(), local_ty) {
        (PhpType::Array(param_elem), PhpType::Array(local_elem)) => {
            let param_elem = param_elem.codegen_repr();
            let local_elem = local_elem.codegen_repr();
            if local_elem == PhpType::Void && param_elem != PhpType::Void {
                return Some((None, PhpType::Array(Box::new(param_elem))));
            }
            (param_elem == PhpType::Mixed && local_elem != PhpType::Mixed).then(|| {
                (
                    Some(Op::ArrayToMixed),
                    PhpType::Array(Box::new(PhpType::Mixed)),
                )
            })
        }
        (
            PhpType::AssocArray {
                value: param_value, ..
            },
            PhpType::AssocArray {
                key: local_key,
                value: local_value,
            },
        ) => {
            let param_value = param_value.codegen_repr();
            let local_value = local_value.codegen_repr();
            if local_value == PhpType::Void && param_value != PhpType::Void {
                return Some((
                    None,
                    PhpType::AssocArray {
                        key: local_key,
                        value: Box::new(param_value),
                    },
                ));
            }
            (param_value == PhpType::Mixed && local_value != PhpType::Mixed).then(|| {
                (
                    Some(Op::HashToMixed),
                    PhpType::AssocArray {
                        key: local_key,
                        value: Box::new(PhpType::Mixed),
                    },
                )
            })
        }
        // An empty LIST the callee fills by KEY has to become a hash before the call: the
        // callee is compiled for bucket storage and would otherwise write string keys into a
        // packed vector. `$h = []; fill_keyed($h);` read back as `[10, 13]` without this.
        (PhpType::AssocArray { .. }, PhpType::Array(local_elem))
            if local_elem.codegen_repr() == PhpType::Void =>
        {
            Some((Some(Op::ArrayToHash), param_ty.codegen_repr()))
        }
        _ => None,
    }
}

/// Lowers positional call arguments with omitted optional defaults and variadic tail packing.
pub(super) fn lower_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    lower_args_with_signature_options(ctx, sig, args, false, false)
}


/// Lowers arguments while preserving omission of trailing default-only parameter slots.
pub(super) fn lower_args_with_signature_trimming_trailing_defaults(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    lower_args_with_signature_options(ctx, sig, args, true, false)
}

/// [`lower_args_with_signature`] knowing the callee PHP would name in a TypeError.
///
/// User-defined functions and methods provide the name; builtins retain their own diagnostics.
pub(super) fn lower_args_with_signature_for(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    callee: Option<&str>,
) -> Vec<crate::ir::ValueId> {
    lower_args_with_signature_options_impl(ctx, sig, args, false, false, None, callee)
}

/// Applies shared argument planning with optional elision of trailing defaults.
pub(super) fn lower_args_with_signature_options(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    trim_trailing_defaults: bool,
    capture_values: bool,
) -> Vec<crate::ir::ValueId> {
    lower_args_with_signature_options_for_capture(
        ctx, sig, args, trim_trailing_defaults, capture_values, None,
    )
}

/// Promotes one runtime-parser output when its argument is evaluated in source order.
pub(super) fn lower_args_with_signature_options_for_capture(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    trim_trailing_defaults: bool,
    capture_values: bool,
    capture_output_index: Option<usize>,
) -> Vec<crate::ir::ValueId> {
    lower_args_with_signature_options_impl(
        ctx, sig, args, trim_trailing_defaults, capture_values, capture_output_index, None,
    )
}

/// Shared argument planning; `callee` names the function php would cite in a TypeError.
fn lower_args_with_signature_options_impl(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    trim_trailing_defaults: bool,
    capture_values: bool,
    capture_output_index: Option<usize>,
    callee: Option<&str>,
) -> Vec<crate::ir::ValueId> {
    let Some(sig) = sig else {
        return lower_args(ctx, args);
    };
    create_by_ref_arg_locals(ctx, sig, args);
    let literal_bound = rewrite_literal_param_bindings(sig, args);
    let args = literal_bound.as_deref().unwrap_or(args);
    if crate::types::call_args::has_named_args(args) {
        let operands = lower_named_args_with_signature_options(
            ctx, sig, args, trim_trailing_defaults, capture_values, capture_output_index,
        );
        return coerce_operands_to_params(ctx, sig, operands);
    }
    if let Some(operands) = lower_positional_spread_args_with_signature(ctx, sig, args, None, capture_values) {
        return operands;
    }
    let static_spread_args = if has_static_call_spread_args(args) {
        Some(expand_static_call_spread_args(args))
    } else {
        None
    };
    let args = static_spread_args.as_deref().unwrap_or(args);
    if let Some(operands) = lower_assoc_spread_only_args(ctx, sig, args) {
        return coerce_operands_to_params(ctx, sig, operands);
    }
    if args.iter().any(is_spread_arg) {
        if capture_values {
            return args.iter().enumerate().map(|(index, arg)| {
                if is_spread_arg(arg) {
                    let lowered = lower_expr(ctx, arg);
                    capture_call_argument_value(ctx, lowered, index, arg.span).value
                } else {
                    lower_arg_with_signature_options_for(ctx, sig, index, arg, true, callee)
                }
            }).collect();
        }
        return lower_args(ctx, args);
    }
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let fixed_arg_count = if sig.variadic.is_some() {
        args.len().min(regular_param_count)
    } else {
        args.len()
    };
    if sig.variadic.is_none() && fixed_arg_count >= regular_param_count {
        let operands = args
            .iter()
            .enumerate()
            .map(|(index, arg)| {
                if capture_output_index == Some(index) {
                    promote_captured_reference_argument(ctx, arg);
                }
                let value =
                    lower_arg_with_signature_options_for(ctx, sig, index, arg, capture_values, callee);
                if !capture_values && index + 1 < args.len() {
                    root_prior_argument_preserving_reference_place(
                        ctx, sig, index, arg, value,
                    )
                } else {
                    value
                }
            })
            .collect();
        return coerce_operands_to_params(ctx, sig, operands);
    }
    let mut operands: Vec<crate::ir::ValueId> = args[..fixed_arg_count]
        .iter()
        .enumerate()
        .map(|(index, arg)| {
            if capture_output_index == Some(index) {
                promote_captured_reference_argument(ctx, arg);
            }
            let value =
                lower_arg_with_signature_options_for(ctx, sig, index, arg, capture_values, callee);
            if !capture_values && index + 1 < args.len() {
                root_prior_argument_preserving_reference_place(
                    ctx, sig, index, arg, value,
                )
            } else {
                value
            }
        })
        .collect();
    if !trim_trailing_defaults {
        for idx in fixed_arg_count..regular_param_count {
            let Some(Some(default)) = sig.defaults.get(idx) else {
                break;
            };
            operands.push(lower_arg_with_signature(ctx, sig, idx, default));
        }
    }
    if sig.variadic.is_some() {
        let tail = if args.len() > regular_param_count {
            &args[regular_param_count..]
        } else {
            &[]
        };
        if crate::func_args::sig_has_hidden_argc_param(sig) {
            operands.push(emit_i64_at_span(ctx, args.len() as i64, crate::span::Span::dummy()).value);
        }
        operands.push(lower_variadic_tail_array(ctx, sig, tail, args.len()).value);
    }
    coerce_operands_to_params(ctx, sig, operands)
}
