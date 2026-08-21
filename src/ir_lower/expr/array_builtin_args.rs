//! Purpose:
//! Array mutation and comparator-specific builtin argument lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `array_push($local, $value)` as a direct indexed-array mutation.
pub(super) fn lower_static_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_push" || args.len() != 2 {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    let ExprKind::Variable(array_name) = &args[0].kind else {
        return None;
    };
    if !matches!(ctx.local_type(array_name).codegen_repr(), PhpType::Array(_)) {
        return None;
    }
    let array_value = ctx.load_local(array_name, Some(args[0].span));
    if array_value.ir_type != IrType::Heap(IrHeapKind::Array) {
        return None;
    }
    let value = lower_expr(ctx, &args[1]);
    let (array_value, updated_ty, needs_storeback) =
        if crate::ir_lower::stmt::ref_bound_mixed_indexed_array_write(ctx, array_name, value) {
            (array_value, Some(ctx.local_type(array_name)), true)
        } else {
            crate::ir_lower::stmt::prepare_indexed_array_local_write(ctx, array_value, value, expr.span)
        };
    ctx.emit_void(
        Op::ArrayPush,
        vec![array_value.value, value.value],
        None,
        Op::ArrayPush.default_effects(),
        Some(expr.span),
    );
    let elem_ty =
        crate::ir_lower::stmt::indexed_array_write_element_type(ctx, array_value, updated_ty.as_ref());
    crate::ir_lower::stmt::finish_indexed_array_local_write(
        ctx,
        array_name,
        array_value,
        updated_ty,
        needs_storeback,
        expr.span,
    );
    crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, expr.span);
    Some(lower_null(ctx, expr))
}

/// Converts a packed indexed receiver of a KEY-PRESERVING sort to int-keyed hash storage.
///
/// php sorts with `zend_array_sort(Z_ARRVAL_P(array), <comparator>, renumber)`
/// (ext/standard/array.c). `sort()` passes `renumber = 1`; `natsort()` and `natcasesort()` pass
/// `0`, so php moves only the iteration order and leaves every key on its own value:
/// `natsort(["img12","img2"])` yields `{"1":"img2","0":"img12"}`, measured under `php -n` 8.5.6. A
/// packed array stores its keys implicitly as slot positions `0..n-1`, which cannot express that
/// permutation, so the receiver's STORAGE has to become a hash before the sort runs. Once it is
/// one, the backend's existing `__rt_hash_natsort` / `__rt_hash_natcasesort` relinkers — which
/// rewrite only the table's iteration chain — deliver php's answer with no new runtime code.
///
/// The conversion is the same `Op::ArrayToHash` pairing `lower_string_key_array_promotion` uses for
/// `$a["k"] = …` on a packed local: release the boxed owner, convert, store the result back. It is
/// emitted HERE, before the argument is lowered, so the by-reference receiver the call loads is
/// already the hash; and because it goes through `set_local_type`, the statement-level
/// representation fixed point sees the conversion and hoists it above any branch that hides it.
/// `Op::ArrayToHash` is idempotent, so a hoisted copy plus this one stay correct.
///
/// Which receivers move is `crate::types::key_preserving_sort_promotes`, the SAME predicate the
/// checker's `promote_indexed_local_for_key_preserving_sort` applies to the type environment: if
/// the two disagreed, one side would compile the local's later reads against a layout the other
/// never built.
pub(super) fn promote_key_preserving_sort_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
) {
    if args.len() != 1
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return;
    }
    let ExprKind::Variable(local) = &args[0].kind else {
        return;
    };
    // A local that ALIASES other storage (`$s = &$r`, a `&$a` parameter) keeps the packed layout:
    // converting it rewrites storage every other name for it is still compiled against, which then
    // reads the hash header as elements — measured, `$r = ["img12","img2"]; $s = &$r; natsort($s);
    // json_encode($r)` printed `[5,0]`. The checker skips the same locals through
    // `active_ref_params`, so the two never disagree about what the receiver's storage is.
    if ctx.is_ref_bound_local(local) {
        return;
    }
    // Matched on the local's own recorded type, not on `codegen_repr()`, because the checker's
    // half matches the environment entry the same way: a shape that only COLLAPSES to `Array` is
    // promoted by neither, so the two can never end up compiling the local against different
    // storage.
    let PhpType::Array(elem_ty) = ctx.local_type(local) else {
        return;
    };
    if !crate::types::key_preserving_sort_promotes(name, &elem_ty) {
        return;
    }
    let span = args[0].span;
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: elem_ty,
    };
    let array_value = ctx.load_local(local, Some(span));
    ctx.prepare_mutated_local_owner(local, array_value, assoc_ty.clone(), Some(span));
    let hash = ctx.emit_value(
        Op::ArrayToHash,
        vec![array_value.value],
        None,
        assoc_ty.clone(),
        Op::ArrayToHash.default_effects(),
        Some(span),
    );
    ctx.store_prepared_mutated_local(local, hash, assoc_ty, Some(span));
}

/// Lowers `sort($local)` / `rsort($local)` on an `array|false`-union local as unbox-then-sort.
///
/// `$d = scandir($dir); sort($d);` stores a BOXED value, and the by-reference receiver path
/// hands codegen a Mixed-typed operand the sort dispatch refuses. The union is only visible
/// HERE, at lowering time: the local is loaded, unboxed through the same `ExpectArrayArg`
/// wrap the positional family uses (a runtime `false` throws php's TypeError, worded as php
/// words it), and the raw array is sorted IN PLACE — so the box keeps pointing at the sorted
/// storage and the local needs no write-back. php's `sort()` returns `true`; the sort
/// builtins here are declared `Void`, so the call's value is the shared null sentinel,
/// matching every other sort call site.
pub(super) fn lower_union_array_in_place_sort(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let canonical = php_symbol_key(name.trim_start_matches('\\'));
    let runtime = match canonical.as_str() {
        "sort" => crate::ir::RuntimeFnId::Sort,
        "rsort" => crate::ir::RuntimeFnId::Rsort,
        _ => return None,
    };
    if args.len() != 1 || crate::types::call_args::has_named_args(args) {
        return None;
    }
    let ExprKind::Variable(local) = &args[0].kind else {
        return None;
    };
    let local_ty = ctx.local_type(local);
    local_ty.array_or_false_member()?;
    // The unbox-or-throw wrap hands the sort machinery a RAW array typed with the union's
    // member (a runtime `false` throws php's TypeError first). The box is its array's sole
    // owner, so the copy-on-write split sees refcount 1 and leaves the storage in place: the
    // in-place sort is visible through the box, and the local needs no write-back.
    let loaded = ctx.load_local(local, Some(args[0].span));
    let mut values = vec![loaded.value];
    wrap_array_or_false_args_impl(ctx, &canonical, Some("array"), 0, args, &mut values);
    ctx.emit_void(
        Op::RuntimeCall,
        values,
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(runtime))),
        Op::RuntimeCall.default_effects(),
        Some(expr.span),
    );
    Some(lower_null(ctx, expr))
}


/// Wraps `array|false` union arguments to array-taking builtins in an unbox-or-throw call.
///
/// The wrapped value is a raw array pointer typed with the union's array member, so the
/// consumer's lowering is untouched; a runtime `false` throws php's TypeError with the exact
/// message php composes, built here at compile time.
fn wrap_array_or_false_args(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    args: &[Expr],
    values: &mut [crate::ir::ValueId],
) {
    for &(name, index, param) in crate::builtins::array_or_false::ARRAY_OR_FALSE_ARG_SITES {
        if name != canonical {
            continue;
        }
        wrap_array_or_false_args_impl(ctx, name, param, index, args, values);
    }
}

/// Wraps ONE argument slot in the unbox-or-throw call when it carries an `array|false` union.
fn wrap_array_or_false_args_impl(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    param: Option<&str>,
    index: usize,
    args: &[Expr],
    values: &mut [crate::ir::ValueId],
) {
    let Some(&value) = values.get(index) else {
        return;
    };
    let ty = ctx.builder.value_php_type(value);
    let Some(member) = ty.array_or_false_member().cloned() else {
        return;
    };
    let span = args.get(index).map(|arg| arg.span);
    // A variadic argument has no name in php's wording: `array_merge(): Argument #2 must be
    // of type array, false given` — measured, no `($name)` segment.
    let message = match param {
        Some(param) => format!(
            "{}(): Argument #{} (${}) must be of type array, false given",
            name,
            index + 1,
            param
        ),
        None => format!(
            "{}(): Argument #{} must be of type array, false given",
            name,
            index + 1
        ),
    };
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
            span,
        )
        .expect("const_str produces a value");
    // BORROWED, not owned: the unboxed pointer is the box's own storage, and the consuming
    // call's owned-argument release would otherwise free the array UNDER the box — measured as
    // `sort($d)` sorting freed memory after an earlier `in_array(..., $d)` had "released" it.
    let member_ir = crate::ir_lower::context::return_ir_type(&member);
    let wrapped = ctx
        .builder
        .emit_with_effects(
            Op::RuntimeCall,
            vec![value, message],
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
                crate::builtins::array_or_false::EXPECT_ARRAY_ARG,
            ))),
            member_ir,
            member,
            Ownership::Borrowed,
            Op::RuntimeCall.default_effects(),
            span,
        )
        .expect("expect_array_arg produces a value");
    values[index] = wrapped;
}

/// Lowers builtin call operands, applying builtin-specific preservation where source order matters.
pub(super) fn lower_builtin_call_args(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    if is_empty_static_indexed_spread_arg(args) && zero_arity_call_signature(name, sig) {
        return Vec::new();
    }
    let canonical = php_symbol_key(name.trim_start_matches('\\'));
    if canonical == "eval" {
        return lower_eval_args(ctx, sig, args);
    }
    let argument_lowering = crate::builtins::registry::lookup(&canonical)
        .map(|def| def.spec.semantics.argument_lowering)
        .unwrap_or(crate::builtins::semantics::BuiltinArgumentLowering::Standard);
    match argument_lowering {
        crate::builtins::semantics::BuiltinArgumentLowering::Count => {
            lower_count_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::Date => {
            lower_date_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::JsonDecode => {
            lower_json_decode_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::PregReplaceCallback
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_preg_replace_callback_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::PositionalRegex
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_args(ctx, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::UserValueSort
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_user_value_sort_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::OpensslEncrypt => {
            prepare_openssl_encrypt_tag_local(ctx, args);
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg)
            {
                lower_positional_builtin_args_with_signature(ctx, sig, args)
            } else {
                lower_args_with_signature(ctx, sig, args)
            }
        }
        crate::builtins::semantics::BuiltinArgumentLowering::ArraySplice
            if !args.iter().any(is_spread_arg) =>
        {
            lower_array_splice_args(ctx, sig, args)
        }
        _ if !crate::types::call_args::has_named_args(args)
            && !args.iter().any(is_spread_arg) =>
        {
            if let Some(values) =
                lower_write_only_variadic_builtin_args(ctx, &canonical, sig, args)
            {
                return values;
            }
            let mut values = lower_positional_builtin_args_with_signature(ctx, sig, args);
            // Named/spread spellings skip the wrap and fail the consumer's own gate loudly at
            // compile time — an honest refusal, where the wrap's absence at RUN time would
            // have been a boxed cell read as an array.
            wrap_array_or_false_args(ctx, &canonical, args, &mut values);
            values
        }
        _ => lower_args_with_signature(ctx, sig, args),
    }
}

/// Promotes the OpenSSL encrypt tag target to string-capable storage before lowering its load.
fn prepare_openssl_encrypt_tag_local(ctx: &mut LoweringContext<'_, '_>, args: &[Expr]) {
    let expanded = crate::types::call_args::expand_static_assoc_spread_args(args);
    let tag = expanded
        .iter()
        .find_map(|arg| match &arg.kind {
            ExprKind::NamedArg { name, value } if php_symbol_key(name) == "tag" => {
                Some(value.as_ref())
            }
            _ => None,
        })
        .or_else(|| {
            expanded
                .get(5)
                .filter(|arg| !matches!(arg.kind, ExprKind::NamedArg { .. }))
        });
    let Some(Expr {
        kind: ExprKind::Variable(name),
        ..
    }) = tag
    else {
        return;
    };
    ctx.set_local_type(name, PhpType::Str);
}

/// Lowers plain positional builtin operands without materializing omitted defaults or packing tails.
///
/// Runtime helpers consume the caller-provided arity, while the registry signature still supplies
/// by-reference handling and scalar storage coercions for every visible regular parameter.
pub(super) fn lower_positional_builtin_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let Some(sig) = sig else {
        return lower_args(ctx, args);
    };
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    args.iter()
        .enumerate()
        .map(|(index, arg)| {
            if index < regular_param_count {
                lower_arg_with_signature(ctx, sig, index, arg)
            } else {
                lower_expr(ctx, arg).value
            }
        })
        .collect()
}


/// Lowers a builtin whose VARIADIC TAIL is write-only, auto-vivifying each output variable.
///
/// `sscanf($s, '%d %s', $n, $w)` fills both variables and neither has to exist beforehand — php
/// binds an undeclared variable to a by-reference parameter by materializing it as `null` first.
/// Two things go wrong without that. An undeclared caller slot holds whatever the frame held, and
/// the callee's store into a `mixed` reference releases the previous occupant, so the call
/// SEGFAULTED on a garbage pointer. A declared one holding `null` fares no better: the lowering
/// keeps its own local-type map, so the load after the call still read `php=null` and every use
/// of it constant-folded to `NULL` while the write went through the pointer unseen.
///
/// Storing a freshly boxed `null` fixes both at once: the slot is initialized AND re-typed, which
/// is exactly what `variadic_writes` promises about the tail.
fn lower_write_only_variadic_builtin_args(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Option<Vec<crate::ir::ValueId>> {
    let def = crate::builtins::registry::lookup(canonical)?;
    let written = def.spec.variadic_writes?;
    let sig = sig?;
    let regular = crate::types::call_args::regular_param_count(sig);
    if args.len() <= regular {
        return None;
    }
    let written = crate::builtins::convert::type_spec_to_php(&written);
    let mut values = Vec::with_capacity(args.len());
    for (index, arg) in args.iter().enumerate() {
        if index < regular {
            values.push(lower_arg_with_signature(ctx, sig, index, arg));
            continue;
        }
        values.push(lower_write_only_out_arg(ctx, arg, &written));
    }
    Some(values)
}

/// Materializes one write-only output variable as a fresh `null` in the caller's storage.
fn lower_write_only_out_arg(
    ctx: &mut LoweringContext<'_, '_>,
    arg: &Expr,
    written: &PhpType,
) -> crate::ir::ValueId {
    let ExprKind::Variable(name) = &arg.kind else {
        // Anything that is not a plain variable has no slot to write back through; the checker
        // rejects those separately, and lowering it as a value keeps that diagnostic the one the
        // caller sees.
        return lower_expr(ctx, arg).value;
    };
    let null = ctx.emit_value(
        Op::ConstNull,
        Vec::new(),
        None,
        PhpType::Void,
        Op::ConstNull.default_effects(),
        Some(arg.span),
    );
    let initial = if matches!(written.codegen_repr(), PhpType::Mixed) {
        ctx.emit_value(
            Op::MixedBox,
            vec![null.value],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(arg.span),
        )
    } else {
        null
    };
    ctx.store_call_normalized_local(name, initial, written.clone(), Some(arg.span));
    ctx.load_local(name, Some(arg.span)).value
}

/// Lowers `count()` arguments, dropping a statically-default mode argument.
///
/// The EIR backend implements only `COUNT_NORMAL`; a literal `0` mode (named
/// or positional) is semantically a no-op and would otherwise trip the unary
/// count contract in codegen.
pub(super) fn lower_count_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let pruned: Vec<Expr> = args
        .iter()
        .enumerate()
        .filter(|(index, arg)| !count_arg_is_static_default_mode(*index, arg))
        .map(|(_, arg)| arg.clone())
        .collect();
    let mut operands = lower_args_with_signature(ctx, sig, &pruned);
    // Named and spread plans re-materialize the optional `mode` default even
    // after the AST prune; a trailing constant-zero mode stays a no-op for
    // the unary count contract, so drop the operand (DCE reclaims the const).
    if operands.len() == 2 {
        let trailing_zero_mode = ctx
            .builder
            .value_defining_instruction(operands[1])
            .is_some_and(|inst| {
                inst.op == Op::ConstI64
                    && matches!(inst.immediate, Some(crate::ir::Immediate::I64(0)))
            });
        if trailing_zero_mode {
            operands.pop();
        }
    }
    operands
}

/// Returns true when a `count()` argument is a statically-zero mode.
pub(super) fn count_arg_is_static_default_mode(index: usize, arg: &Expr) -> bool {
    match &arg.kind {
        ExprKind::NamedArg { name, value } => {
            name == "mode" && matches!(value.kind, ExprKind::IntLiteral(0))
        }
        ExprKind::IntLiteral(0) => index == 1,
        _ => false,
    }
}

/// Lowers eval's code operand and coerces it through PHP string-conversion rules.
pub(super) fn lower_eval_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let operands = lower_args_with_signature(ctx, sig, args);
    let Some(code) = operands.first().copied() else {
        return operands;
    };
    let code_value = LoweredValue {
        value: code,
        ir_type: ctx.builder.value_type(code),
    };
    let span = args.first().map(|arg| arg.span);
    vec![coerce_to_string_at_span(ctx, code_value, span).value]
}

/// Lowers `usort`/`uasort` arguments, typing an unannotated comparator closure
/// against the array's object element type.
///
/// `usort`/`uasort` compare values, so a comparator over an array of objects must
/// see each element as the object handle — for `<=>` instant comparison and for
/// property/method access — not the raw pointer-sized integer the runtime stores
/// in each slot. The array operand is lowered exactly as the default positional
/// path would (positional builtin calls reach here with no signature); only an
/// unannotated closure comparator over an object-element array is specialized,
/// matching the element-type hint the checker applied to the comparator body.
pub(super) fn lower_user_value_sort_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    if args.len() != 2 || !matches!(&args[1].kind, ExprKind::Closure { .. }) {
        return lower_args_with_signature(ctx, sig, args);
    }
    // The mutating sort keeps its by-reference local storeback in the EIR backend,
    // so the array operand only has to resolve to the array's value here.
    let array = match sig {
        Some(sig) => lower_arg_with_signature(ctx, sig, 0, &args[0]),
        None => lower_expr(ctx, &args[0]).value,
    };
    let elem_ty = match ctx.builder.value_php_type(array).codegen_repr() {
        PhpType::Array(elem) => elem.codegen_repr(),
        _ => PhpType::Int,
    };
    // Only an object-element array needs the comparator parameters re-typed; scalar
    // comparators already lower correctly through the default path.
    let callback = if matches!(elem_ty, PhpType::Object(_)) {
        lower_value_sort_comparator_closure(ctx, &args[1], elem_ty)
    } else {
        match sig {
            Some(sig) => lower_arg_with_signature(ctx, sig, 1, &args[1]),
            None => lower_expr(ctx, &args[1]).value,
        }
    };
    vec![array, callback]
}

/// Lowers a value-sort comparator closure with both parameters typed as the array element.
///
/// Falls back to the plain closure lowering for any non-closure callback operand,
/// though callers only reach this path with a closure comparator.
pub(super) fn lower_value_sort_comparator_closure(
    ctx: &mut LoweringContext<'_, '_>,
    callback: &Expr,
    elem_ty: PhpType,
) -> crate::ir::ValueId {
    let ExprKind::Closure {
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        captures,
        capture_refs,
        is_static,
        ..
    } = &callback.kind
    else {
        return lower_expr(ctx, callback).value;
    };
    lower_closure_with_context(
        ctx,
        params,
        variadic.as_deref(),
        *variadic_by_ref,
        return_type.as_ref(),
        body,
        captures,
        capture_refs,
        callback,
        &[elem_ty.clone(), elem_ty],
        None,
        *is_static,
    )
    .value
}
