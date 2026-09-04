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
    args.iter().map(|arg| lower_expr(ctx, arg).value).collect()
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
    if let Some(value) = lower_by_ref_array_element_arg_with_signature(ctx, sig, index, arg) {
        return value;
    }
    if let Some(value) = lower_by_ref_array_arg_with_signature(ctx, sig, index, arg) {
        return value;
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
    if !ctx.local_name_is_undefined(name) {
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
        CastType::Int | CastType::Float | CastType::Array => value,
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
            operands[index] = coerce_to_float_at_span(ctx, lowered, None).value;
        } else if param_ty == PhpType::Str
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_))
        {
            let lowered = LoweredValue {
                value,
                ir_type: ctx.builder.value_type(value),
            };
            operands[index] = coerce_to_string_at_span(ctx, lowered, None).value;
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
                operands[index] = apply_scalar_param_cast(ctx, cast, lowered, None).value;
            }
        }
    }
    operands
}

/// Widens local indexed-array storage before passing it to an `array<mixed>` ref parameter.
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
    let PhpType::Array(elem_ty) = ctx.local_type(array_name).codegen_repr() else {
        return None;
    };
    let (_, param_ty) = sig.params.get(index)?;
    let element_ty = match normalize_value_php_type(*elem_ty) {
        PhpType::Void => normalize_value_php_type(param_ty.codegen_repr()),
        other => other,
    };
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
            element_ty,
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
/// Lowers positional call arguments with omitted optional defaults and variadic tail packing.
pub(super) fn lower_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    lower_args_with_signature_for(ctx, sig, args, None)
}

/// [`lower_args_with_signature`] knowing the callee php would NAME in an argument TypeError.
///
/// Only the three USER-call sites — a function, an instance method, a static method — pass one.
/// A builtin composes its own refusals, so it keeps the unnamed spelling.
pub(super) fn lower_args_with_signature_for(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    callee: Option<&str>,
) -> Vec<crate::ir::ValueId> {
    let Some(sig) = sig else {
        return lower_args(ctx, args);
    };
    create_by_ref_arg_locals(ctx, sig, args);
    let literal_bound = rewrite_literal_param_bindings(sig, args);
    let args = literal_bound.as_deref().unwrap_or(args);
    if crate::types::call_args::has_named_args(args) {
        let operands = lower_named_args_with_signature(ctx, sig, args);
        return coerce_operands_to_params(ctx, sig, operands);
    }
    if let Some(operands) = lower_positional_spread_args_with_signature(ctx, sig, args) {
        return coerce_operands_to_params(ctx, sig, operands);
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
            .map(|(index, arg)| lower_arg_with_signature_for(ctx, sig, index, arg, callee))
            .collect();
        return coerce_operands_to_params(ctx, sig, operands);
    }
    let mut operands: Vec<crate::ir::ValueId> = args[..fixed_arg_count]
        .iter()
        .enumerate()
        .map(|(index, arg)| lower_arg_with_signature_for(ctx, sig, index, arg, callee))
        .collect();
    for idx in fixed_arg_count..regular_param_count {
        let Some(Some(default)) = sig.defaults.get(idx) else {
            break;
        };
        operands.push(lower_expr(ctx, default).value);
    }
    if sig.variadic.is_some() {
        let tail = if args.len() > regular_param_count {
            &args[regular_param_count..]
        } else {
            &[]
        };
        operands.push(lower_variadic_tail_array(ctx, sig, tail).value);
    }
    coerce_operands_to_params(ctx, sig, operands)
}
