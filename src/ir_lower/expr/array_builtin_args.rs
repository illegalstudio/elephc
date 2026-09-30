//! Purpose:
//! Array mutation and comparator-specific builtin argument lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `array_push($local, $value…)` as direct indexed-array mutations.
///
/// The fast path for the common receiver: a plain local already typed as an indexed array, where
/// the append can be emitted inline instead of going through the `runtime.array_push` call. Any
/// number of values is accepted, matching PHP's `array_push(array &$array, mixed ...$values)`;
/// each is appended in source order through its own `ArrayPush`, because an append may relocate
/// the array and the surrounding write-back has to run between values rather than once at the
/// end.
///
/// `array_push($a)` with no values is legal PHP and reads the length straight back.
pub(super) fn lower_static_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_push" || args.is_empty() {
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
    if ctx.load_local(array_name, Some(args[0].span)).ir_type != IrType::Heap(IrHeapKind::Array) {
        return None;
    }
    // Every value is lowered BEFORE the first append. PHP evaluates a call's arguments and only
    // then enters the function, so nothing an argument reads may observe an append this same
    // call performs: `$a = [10]; array_push($a, 1, count($a));` appends `1`, not `2`.
    // Interleaving the two loops was invisible while the arity was pinned at one value.
    let values: Vec<LoweredValue> = args[1..].iter().map(|arg| lower_expr(ctx, arg)).collect();
    for value in values {
        // Re-read the local for every append: an earlier one may have replaced the slot's
        // pointer, and appending into the stale one would write to freed storage.
        let array_value = ctx.load_local(array_name, Some(args[0].span));
        let (array_value, updated_ty, needs_storeback) =
            if crate::ir_lower::stmt::ref_bound_mixed_indexed_array_write(ctx, array_name, value) {
                (array_value, Some(ctx.local_type(array_name)), true)
            } else {
                crate::ir_lower::stmt::prepare_indexed_array_local_write(
                    ctx,
                    array_value,
                    value,
                    expr.span,
                )
            };
        ctx.emit_void(
            Op::ArrayPush,
            vec![array_value.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(expr.span),
        );
        let elem_ty = crate::ir_lower::stmt::indexed_array_write_element_type(
            ctx,
            array_value,
            updated_ty.as_ref(),
        );
        crate::ir_lower::stmt::finish_indexed_array_local_write(
            ctx,
            array_name,
            array_value,
            updated_ty,
            needs_storeback,
            expr.span,
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(
            ctx,
            elem_ty.as_ref(),
            value,
            expr.span,
        );
    }
    // PHP returns the new element count. Reading it from the final array rather than tracking it
    // across the appends also gives the value-less form its answer for free.
    let array_value = ctx.load_local(array_name, Some(args[0].span));
    Some(ctx.emit_value(
        Op::ArrayLen,
        vec![array_value.value],
        None,
        PhpType::Int,
        Op::ArrayLen.default_effects(),
        Some(expr.span),
    ))
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


/// Creates the variables a builtin's BY-REFERENCE parameters are about to bind.
///
/// `stream_socket_server($address, $errno, $errstr, ...)` names `$errno` and `$errstr` for the
/// callee to write into, and PHP creates them there rather than reading them: MEASURED on
/// `php -n` 8.5.6, which raises nothing for either, against the same two names read one line
/// earlier, which do warn. Without this they were ordinary reads, and `examples/udp-socket` and
/// `examples/udg-socket` each printed two warnings PHP does not.
///
/// The predicate is `by_ref`, not `writes`: `writes` marks the narrower write-ONLY subset
/// (`ref(Int) error_code`), while `preg_match`'s `$matches` is a plain `ref` the callee also
/// reads. PHP creates the variable for both, so the wider flag is the right one. Either way the
/// positions come from the registry, the single source `out_params` reads too, so the checker's
/// idea of which parameters are out-parameters and the lowering's cannot drift apart.
fn create_by_ref_arg_locals(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    args: &[Expr],
) {
    let Some(def) = crate::builtins::registry::lookup(canonical) else {
        return;
    };
    for (index, arg) in args.iter().enumerate() {
        let (target, param) = match &arg.kind {
            ExprKind::NamedArg { name, value } => (
                value.as_ref(),
                def.spec.params.iter().find(|param| param.name == name),
            ),
            _ => (arg, def.spec.params.get(index)),
        };
        let binds_by_ref = match param {
            Some(param) => param.by_ref,
            None => index >= def.spec.params.len() && def.spec.variadic_writes.is_some(),
        };
        if !binds_by_ref {
            continue;
        }
        let ExprKind::Variable(name) = &target.kind else {
            continue;
        };
        let declared = param.map(|param| crate::builtins::convert::builtin_param_php_type(def.spec, &param.ty));
        // A callee that FILLS the caller's array needs one handed over, not the boxed null every
        // other out-parameter starts as — and afterwards the local holds what it filled it with.
        // Both halves are the same shared answer the checker binds its own view to.
        let filled = crate::builtins::by_ref_fill::filled_array_arg_type(canonical, index);
        create_by_ref_arg_local(ctx, name, declared.as_ref(), target, filled.clone());
        if let Some(filled) = filled {
            ctx.set_local_type(name, filled);
        }
    }
}

/// Lowers builtin call operands, applying builtin-specific preservation where source order matters.
pub(super) fn lower_builtin_call_args(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let mut values = lower_builtin_call_operands(ctx, name, sig, args);
    let canonical = php_symbol_key(name.trim_start_matches('\\'));
    coerce_null_operands_to_builtin_params(ctx, &canonical, args, &mut values);
    values
}

/// Replaces a NULL operand with what PHP coerces it to for an INTERNAL function's scalar
/// parameter.
///
/// php-src coerces null into a non-nullable scalar parameter of an internal function rather
/// than refusing: `strlen(null)` answers `int(0)` and `ord(null)` answers `int(0)`, each after
/// a `Passing null to parameter #N ... is deprecated` notice. USER functions do NOT get this —
/// `function f(int $v) {} f(null);` is an uncaught `TypeError` — which is why this is keyed to
/// the builtin registry and not to the shared argument coercion.
///
/// Without it a null operand reached the builtin's own EIR lowering, which has no null case
/// and PANICKED the compiler: `if ($c) { $x = "a"; } strlen($x);` — a program `php -n` 8.5.6
/// runs to `int(0)` — died with `strlen cannot lower checked operand type Void`. Adopting PHP's
/// semantics for undefined variables is what made that reachable; a conditionally assigned
/// variable is null on the path that skips the assignment, exactly as PHP says.
///
/// The operand it replaces is left in place and still runs: it is the `warned_null`
/// that raises `Warning: Undefined variable $x`, which PHP raises here too. Only the VALUE
/// handed to the builtin changes.
///
/// The DEPRECATION notice is not emitted — a measured, separate gap that elephc has for an
/// explicit `null` argument as well.
/// Builtins whose php implementation reads `ZEND_NUM_ARGS()`, so an explicit trailing `null` is
/// php-visibly different from an omitted argument and the operand must not be dropped.
///
/// MEASURED on `php -n` 8.5.6, and the whole difference is the deprecation:
///
/// ```text
/// stream_context_set_option($c, ['http' => [...]])        E_DEPRECATED, then bool(true)
/// stream_context_set_option($c, ['http' => [...]], null)  bool(true), and NO deprecation
/// ```
///
/// This is a list because nothing in the contract can express it: ZPP hands a `?T $x = null`
/// parameter a value plus an `is_null` flag, and whether a given C implementation consults that
/// flag or the argument count is a property of its BODY. A name belongs here only with a
/// measurement like the one above showing the two spellings differ.
const ARITY_SENSITIVE_BUILTINS: &[&str] = &["stream_context_set_option"];

fn coerce_null_operands_to_builtin_params(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    args: &[Expr],
    values: &mut Vec<crate::ir::ValueId>,
) {
    let Some(def) = crate::builtins::registry::lookup(canonical) else {
        return;
    };
    // A TRAILING null on a `?T $x = null` parameter is php's OMITTED argument, not a coerced
    // scalar: `substr("hello", 1, null)` answers `"ello"` and `umask(null)` reads the mask,
    // exactly as the shorter call does. Dropping the operand routes each builtin to the
    // omitted-argument branch its own lowering already has, instead of asking 33 lowerings to
    // learn a null case. MEASURED against `php -n` 8.5.6: coercing instead answered `""` for that
    // `substr`, `""` for `stream_get_contents($h, null)`, and copied nothing for
    // `stream_copy_to_stream($a, $b, null)`.
    //
    // Only from the END: a null in a middle position — `stream_copy_to_stream($a, $b, null, 4)` —
    // still has to reach the builtin, where the site's own "no bound" word is materialised.
    //
    // Only for a DECLARED SCALAR, which is where php's own two spellings coincide. For a `mixed`
    // parameter they do not: `stream_filter_append($h, "convert.base64-encode",
    // STREAM_FILTER_WRITE, null)` answers `false` while the same call with the argument OMITTED
    // answers a resource, because php's `convert.*` filters test the zval POINTER and it is null
    // only when nothing was supplied. ZPP gives a `?int $x = null` parameter a value plus an
    // `is_null` flag instead, and every builtin here reads that flag as "take the default".
    //
    // The dropped operand's INSTRUCTION stays in the IR and still runs, which is what keeps a
    // `warned_null` argument raising its `Warning: Undefined variable` before the call.
    //
    // And not at all for a builtin that reads the argument COUNT rather than the flag, because
    // for those the two spellings are php-visibly different — see `ARITY_SENSITIVE_BUILTINS`.
    while !ARITY_SENSITIVE_BUILTINS.contains(&canonical) {
        let Some(last) = values.len().checked_sub(1) else {
            break;
        };
        let Some(param) = def.spec.params.get(last) else {
            break;
        };
        if param.by_ref
            || !matches!(param.default, Some(crate::builtins::spec::DefaultSpec::Null))
            || !matches!(
                crate::builtins::convert::builtin_param_php_type(def.spec, &param.ty),
                PhpType::Int | PhpType::Str | PhpType::Float | PhpType::Bool
            )
        {
            break;
        }
        if !matches!(ctx.builder.value_php_type(values[last]), PhpType::Void) {
            break;
        }
        values.pop();
    }
    for (index, value) in values.iter_mut().enumerate() {
        let Some(param) = def.spec.params.get(index) else {
            break;
        };
        if param.by_ref {
            continue;
        }
        // A parameter php spells `?T $x = null` accepts null as a VALUE; only a non-nullable
        // scalar gets the coercion this function is named for. Reaching here means the null is
        // not trailing, so it could not be dropped above and the builtin's own lowering sees it.
        if matches!(param.default, Some(crate::builtins::spec::DefaultSpec::Null)) {
            continue;
        }
        if !matches!(ctx.builder.value_php_type(*value), PhpType::Void) {
            continue;
        }
        let Some(arg) = args.get(index) else {
            break;
        };
        let coerced = match crate::builtins::convert::builtin_param_php_type(def.spec, &param.ty) {
            PhpType::Str => lower_string_literal(ctx, "", arg),
            PhpType::Int => lower_int_literal(ctx, 0, arg),
            PhpType::Float => lower_float_literal(ctx, 0.0, arg),
            PhpType::Bool => lower_bool_literal(ctx, false, arg),
            _ => continue,
        };
        *value = coerced.value;
    }
}

/// Lowers builtin call operands before PHP's null-to-scalar parameter coercion is applied.
fn lower_builtin_call_operands(
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
    create_by_ref_arg_locals(ctx, &canonical, args);
    let argument_lowering = crate::builtins::registry::lookup(&canonical)
        .map(|def| def.spec.semantics.argument_lowering)
        .unwrap_or(crate::builtins::semantics::BuiltinArgumentLowering::Standard);
    let pcntl_outputs = prepare_pcntl_output_locals(ctx, &canonical, sig, args);
    if matches!(argument_lowering,
        crate::builtins::semantics::BuiltinArgumentLowering::Standard
        | crate::builtins::semantics::BuiltinArgumentLowering::MaterializeDefaults
        | crate::builtins::semantics::BuiltinArgumentLowering::PreserveValues
    ) {
        if let Some(sig) = sig {
            if let Some(operands) = dynamic_spreads::lower_boxed_spread_args(ctx, sig, args, name,
                argument_lowering == crate::builtins::semantics::BuiltinArgumentLowering::PreserveValues) {
                return operands;
            }
        }
    }
    if !crate::types::call_args::has_named_args(args)
        && !matches!(argument_lowering,
            crate::builtins::semantics::BuiltinArgumentLowering::PcntlPreserveOmitted
            | crate::builtins::semantics::BuiltinArgumentLowering::PreserveValues)
    {
        if let Some(sig) = sig {
            if let Some(operands) = lower_positional_spread_args_with_signature(
                ctx, sig, args, Some(name), false,
            ) {
                for (name, ty) in pcntl_outputs {
                    ctx.set_local_logical_type(&name, ty);
                }
                return operands;
            }
        }
    }
    let lowered = match argument_lowering {
        crate::builtins::semantics::BuiltinArgumentLowering::PreserveValues => {
            lower_builtin_args_preserving_values(ctx, &canonical, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::MaterializeDefaults => {
            lower_args_with_signature(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::Count => {
            lower_count_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::Date => {
            lower_date_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::JsonDecode => {
            lower_json_decode_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::Getenv => {
            lower_getenv_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::PcntlPreserveOmitted => {
            let writeback_sig = pcntl_writeback_signature(&canonical, sig);
            lower_args_with_signature_trimming_trailing_defaults(
                ctx,
                writeback_sig.as_ref().or(sig),
                args,
            )
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
            args.iter()
                .enumerate()
                .map(|(index, arg)| {
                    let value = lower_expr(ctx, arg);
                    if index + 1 < args.len()
                        && !sig.is_some_and(|sig| {
                            sig.ref_params.get(index).copied().unwrap_or(false)
                        })
                    {
                        root_evaluated_call_argument(ctx, value, arg.span).value
                    } else {
                        value.value
                    }
                })
                .collect()
        }
        crate::builtins::semantics::BuiltinArgumentLowering::UserValueSort
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_user_value_sort_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::ReverseKeySort => {
            lower_reverse_key_sort_args(ctx, sig, args)
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
        crate::builtins::semantics::BuiltinArgumentLowering::XmlHandlerSetter => {
            lower_xml_handler_setter_call(ctx, &canonical, sig, args)
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
    };
    for (name, ty) in pcntl_outputs {
        ctx.set_local_logical_type(&name, ty);
    }
    lowered
}

/// Widens PCNTL output storage before its write-only by-reference loads are lowered.
fn prepare_pcntl_output_locals(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<(String, PhpType)> {
    let mut outputs = Vec::new();
    if !crate::types::call_args::has_named_args(args) && !args.iter().any(is_spread_arg) {
        for (index, arg) in args.iter().enumerate() {
            if let Some(output) = prepare_pcntl_output_local(ctx, canonical, index, arg) {
                outputs.push(output);
            }
        }
        return outputs;
    }
    let Some(sig) = sig else {
        return outputs;
    };
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let Ok(plan) = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        args,
        call_span,
        regular_param_count,
        false,
        true,
        &assoc_spread_sources(ctx, args),
    ) else {
        return outputs;
    };
    for (index, arg) in plan.regular_args.iter().enumerate() {
        let crate::types::call_args::PlannedRegularArg::Source { expr, .. } = arg else {
            continue;
        };
        if let Some(output) = prepare_pcntl_output_local(ctx, canonical, index, expr) {
            outputs.push(output);
        }
    }
    outputs
}

/// Widens one direct PCNTL output slot without reinterpreting its pre-call value.
fn prepare_pcntl_output_local(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    parameter_index: usize,
    value: &Expr,
) -> Option<(String, PhpType)> {
    let ty = pcntl_output_type(canonical, parameter_index)?;
    let ExprKind::Variable(name) = &value.kind else {
        return None;
    };
    if ctx.local_type(name).codegen_repr() != ty.codegen_repr() {
        ctx.set_local_type(name, PhpType::Mixed);
    }
    Some((name.clone(), ty))
}

/// Gives PCNTL write-only outputs their concrete post-call storage type during lowering.
///
/// The PHP contract accepts any pre-call value for these `Mixed` by-reference parameters. Generic
/// reference argument lowering would therefore promote an ordinary output local to a managed Mixed
/// cell, even though the PCNTL backend replaces the value directly and already handles raw,
/// dynamically promoted, and definite ref-cell slots. Refining only the lowering copy prevents the
/// needless promotion while the checker-visible contract remains unchanged.
fn pcntl_writeback_signature(
    canonical: &str,
    sig: Option<&FunctionSig>,
) -> Option<FunctionSig> {
    let mut sig = sig?.clone();
    let mut changed = false;
    for (index, (_, ty)) in sig.params.iter_mut().enumerate() {
        let Some(output_ty) = pcntl_output_type(canonical, index) else {
            continue;
        };
        *ty = output_ty;
        changed = true;
    }
    changed.then_some(sig)
}

/// Returns the concrete value a PCNTL write-only parameter publishes after its call.
fn pcntl_output_type(canonical: &str, parameter_index: usize) -> Option<PhpType> {
    Some(match (canonical, parameter_index) {
        ("pcntl_wait", 0) | ("pcntl_waitpid", 1) => PhpType::Int,
        ("pcntl_wait", 2) | ("pcntl_waitpid", 3) => PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Int),
        },
        ("pcntl_waitid", 2) => PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        },
        ("pcntl_waitid", 4) => PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Int),
        },
        ("pcntl_sigprocmask", 2) => PhpType::Array(Box::new(PhpType::Int)),
        ("pcntl_sigwaitinfo", 1) | ("pcntl_sigtimedwait", 1) => PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        },
        _ => return None,
    })
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
            let value = if index < regular_param_count {
                lower_arg_with_signature(ctx, sig, index, arg)
            } else {
                lower_expr(ctx, arg).value
            };
            if index + 1 < args.len()
                && !sig.ref_params.get(index).copied().unwrap_or(false)
            {
                let lowered = lowered_value_from_id(ctx, value);
                root_evaluated_call_argument(ctx, lowered, arg.span).value
            } else {
                value
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
    let written = crate::builtins::convert::type_spec_to_php_preserving_null(&written);
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

/// Uses shared argument planning without converting values before a runtime-owned parameter parser.
///
/// The storage signature retains names, defaults, arity, and reference modes. Mixed value slots
/// suppress scalar binding without changing the authoritative PHP signature or argument order.
fn lower_builtin_args_preserving_values(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let Some(sig) = sig else {
        return lower_args(ctx, args);
    };
    if name.eq_ignore_ascii_case("mb_convert_variables")
        && !crate::types::call_args::has_named_args(args)
        && !args.iter().any(is_spread_arg)
    {
        return lower_live_mb_convert_variables_args(ctx, args);
    }
    let mut storage = sig.clone();
    for (index, (_, ty)) in storage.params.iter_mut().enumerate() {
        if !storage.ref_params.get(index).copied().unwrap_or(false) {
            *ty = PhpType::Mixed;
        }
    }
    let capture_output_index = crate::builtins::registry::lookup(name)
        .and_then(|def| def.spec.runtime_builtin_id())
        .filter(|id| matches!(id,
            elephc_builtin_contract::RuntimeBuiltinId::MbEreg
                | elephc_builtin_contract::RuntimeBuiltinId::MbEregi
                | elephc_builtin_contract::RuntimeBuiltinId::MbParseStr
                | elephc_builtin_contract::RuntimeBuiltinId::MbConvertVariables))
        .and_then(|id| elephc_builtin_contract::lookup_id(id.builtin_id()))
        .and_then(|contract| contract.params.iter().position(|param| param.by_ref));
    ctx.begin_argument_guard_scope();
    let operands = lower_args_with_signature_options_for_capture(
        ctx, Some(&storage), args, true, true, capture_output_index,
    );
    ctx.end_argument_guard_scope();
    operands
}

/// Keeps each variable's existing reference-cell shape while staging value arguments first.
fn lower_live_mb_convert_variables_args(
    ctx: &mut LoweringContext<'_, '_>, args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    ctx.begin_argument_guard_scope();
    let mut operands = Vec::with_capacity(4);
    for (index, arg) in args.iter().take(2).enumerate() {
        let lowered = lower_expr(ctx, arg);
        operands.push(capture_call_argument_value(ctx, lowered, index, arg.span).value);
    }
    if let Some(root) = args.get(2) {
        if let ExprKind::Variable(name) = &root.kind {
            ctx.promote_local_ref_cell(name, Some(root.span));
        }
        operands.push(lower_expr(ctx, root).value);
    }
    let tail = &args[args.len().min(3)..];
    let array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let array = ctx.emit_value(
        Op::ArrayNew, Vec::new(), Some(Immediate::Capacity(tail.len() as u32)),
        array_ty, Op::ArrayNew.default_effects(),
        tail.first().map(|arg| arg.span),
    );
    for arg in tail {
        let marker = if let ExprKind::Variable(name) = &arg.kind {
            ctx.promote_local_ref_cell(name, Some(arg.span));
            let ty = ctx.local_type(name);
            let slot = ctx.declare_local(name, ty);
            ctx.emit_value(
                Op::InvokerRefArg, Vec::new(), Some(Immediate::LocalSlot(slot)),
                PhpType::Mixed, Op::InvokerRefArg.default_effects(), Some(arg.span),
            )
        } else { lower_expr(ctx, arg) };
        ctx.emit_void(
            Op::ArrayPush, vec![array.value, marker.value], None,
            Op::ArrayPush.default_effects(), Some(arg.span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(
            ctx, Some(&PhpType::Mixed), marker, arg.span,
        );
    }
    operands.push(array.value);
    ctx.end_argument_guard_scope();
    operands
}

/// Preserves a boxed nullable name while reusing shared named and spread argument planning.
fn lower_getenv_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let mut sig = sig.cloned();
    if let Some(sig) = sig.as_mut() {
        crate::ir::RuntimeFnId::Getenv.refine_first_class_callable_sig(sig);
    }
    if !crate::types::call_args::has_named_args(args) && !args.iter().any(is_spread_arg) {
        lower_positional_builtin_args_with_signature(ctx, sig.as_ref(), args)
    } else {
        lower_args_with_signature(ctx, sig.as_ref(), args)
    }
}

/// Promotes a packed local before `krsort()` so descending iteration can preserve integer keys.
///
/// Packed storage has no independent iteration-order metadata: reversing its slots would also
/// change `$array[0]`. Converting the by-reference local to hash storage keeps each key/value
/// pair intact while allowing the runtime helper to reorder only the insertion-order links.
///
/// The backend REFUSES `krsort()` on packed storage rather than guessing, so this promotion is
/// not an optimisation: without it the call cannot be lowered at all.
fn lower_reverse_key_sort_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let Some(sig) = sig else {
        return lower_args(ctx, args);
    };
    let Some(plan) = plan_key_sort_args(sig, args) else {
        return lower_args_with_signature(ctx, Some(sig), args);
    };
    let receiver = plan
        .iter()
        .find_map(|(slot, arg)| (*slot == 0).then_some(arg));
    let Some(receiver) = receiver else {
        return lower_args_with_signature(ctx, Some(sig), args);
    };
    if !is_indexed_array_ref_arg(ctx, sig, 0, receiver) {
        return lower_args_with_signature(ctx, Some(sig), args);
    }
    let mut receiver_value = None;
    let mut flags_value = None;
    for (slot, arg) in &plan {
        if *slot == 0 {
            receiver_value = lower_indexed_array_ref_arg_to_hash(ctx, sig, 0, arg);
        } else {
            flags_value = Some(lower_arg_with_signature(ctx, sig, *slot, arg));
        }
    }
    let Some(receiver_value) = receiver_value else {
        return lower_args_with_signature(ctx, Some(sig), args);
    };
    match flags_value {
        Some(flags) => vec![receiver_value, flags],
        None => vec![receiver_value],
    }
}

/// Binds each written argument of a key sort to its parameter slot, keeping SOURCE order.
///
/// The promotion below has to know which argument is the receiver before it evaluates
/// anything, because `krsort(flags: f(), array: $a)` writes the flag expression first and a
/// late fallback would evaluate `f()` twice.
///
/// The binding itself comes from the shared planner in `src/types/call_args/` rather than
/// being rebuilt here -- named matching, duplicate detection and spread expansion all live
/// there, and a second copy of those rules is free to drift away from what the checker
/// accepted. This only re-reads the plan in source order, which is the one thing the
/// promotion needs that a parameter-indexed plan does not already say.
///
/// Anything the plan does not resolve to written arguments -- a spread that lands on the
/// receiver slot, or a call the planner rejects outright -- returns `None` and falls back to
/// the shared argument path, which owns those shapes and their diagnostics.
pub(super) fn plan_key_sort_args(sig: &FunctionSig, args: &[Expr]) -> Option<Vec<(usize, Expr)>> {
    let span = args.first()?.span;
    let plan = crate::types::call_args::plan_call_args(sig, args, span, false, false).ok()?;
    // A spread has to be evaluated before anything can be said about which element lands on
    // the receiver slot, so it goes to the shared argument path whole.
    if plan.has_spread_args() {
        return None;
    }

    // With no named argument the plan is a passthrough: written order IS parameter order.
    if plan.first_named_pos.is_none() {
        let bound: Vec<(usize, Expr)> = plan.normalized_args().into_iter().enumerate().collect();
        return bound.iter().any(|(slot, _)| *slot == 0).then_some(bound);
    }

    // With one, the plan says which written argument filled each slot; `source_index` is what
    // puts them back in the order they were written, which is the order they must be
    // evaluated in.
    let mut bound: Vec<(usize, usize, Expr)> = Vec::with_capacity(plan.regular_args.len());
    for (slot, planned) in plan.regular_args.iter().enumerate() {
        match planned {
            crate::types::call_args::PlannedRegularArg::Source { source_index, expr } => {
                bound.push((*source_index, slot, expr.clone()));
            }
            // An omitted `$flags` is materialized by the shared default handling below.
            crate::types::call_args::PlannedRegularArg::Default(_) => {}
            crate::types::call_args::PlannedRegularArg::SpreadElement { .. } => return None,
        }
    }
    bound.sort_by_key(|(source_index, _, _)| *source_index);
    let bound: Vec<(usize, Expr)> = bound
        .into_iter()
        .map(|(_, slot, expr)| (slot, expr))
        .collect();
    bound.iter().any(|(slot, _)| *slot == 0).then_some(bound)
}

/// Reports whether `arg` is the packed by-reference local that `krsort()` must promote.
///
/// This mirrors, without evaluating anything, the shape `lower_indexed_array_ref_arg_to_hash`
/// accepts.
fn is_indexed_array_ref_arg(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> bool {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return false;
    }
    let ExprKind::Variable(name) = &arg.kind else {
        return false;
    };
    matches!(ctx.local_type(name).codegen_repr(), PhpType::Array(_))
}

/// Converts one packed by-reference local argument into key-preserving associative storage.
fn lower_indexed_array_ref_arg_to_hash(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<crate::ir::ValueId> {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    let ExprKind::Variable(name) = &arg.kind else {
        return None;
    };
    let PhpType::Array(elem_ty) = ctx.local_type(name).codegen_repr() else {
        return None;
    };
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: elem_ty,
    };
    let array = ctx.load_local(name, Some(arg.span));
    ctx.prepare_mutated_local_owner_for_backend_retire(name, array, assoc_ty.clone(), Some(arg.span));
    let hash = ctx.emit_value(
        Op::ArrayToHash,
        vec![array.value],
        None,
        assoc_ty.clone(),
        Op::ArrayToHash.default_effects(),
        Some(arg.span),
    );
    ctx.store_prepared_mutated_local(name, hash, assoc_ty, Some(arg.span));
    Some(ctx.load_local(name, Some(arg.span)).value)
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
        None,
        *is_static,
    )
    .value
}

#[cfg(test)]
mod tests {
    /// Every `BuiltinArgumentLowering` variant a builtin can declare has an arm that runs it.
    ///
    /// A variant is a four-sided contract: the enum declares it, a builtin selects it, the docs
    /// generator names it, and this file does the work. The origin/main merge resolved the
    /// conflict in this file toward the branch's own copy, which predates upstream's
    /// `ReverseKeySort`, so three sides survived and the one that acts did not. Nothing said so:
    /// the variant simply fell through to the default arm, and `krsort($a)` on a packed array
    /// reached a backend refusal written on the assumption that this lowering had already
    /// promoted the receiver. Fourteen tests failed a long way from the cause.
    ///
    /// The dispatch is a `match` with guards, so it cannot be made exhaustive over the enum and
    /// have the compiler carry this. Reading the two sources is the honest alternative: it costs
    /// nothing and it names the missing side directly. `Standard` is excluded because it IS the
    /// default arm and has no name of its own there.
    #[test]
    fn every_argument_lowering_variant_has_a_dispatch_arm() {
        let declared = include_str!("../../builtins/semantics.rs");
        // Two other lowerings intercept a variant BEFORE it reaches this file's dispatch:
        // `ArrayInternalPointer` is resolved from its descriptor by `expr::mod` and scanned for
        // by `array_pointer_scan`, so it never arrives here and rightly has no arm. Listing
        // them keeps the property "a variant is consumed SOMEWHERE" rather than narrowing it to
        // one file and reporting a correctly-handled variant as missing.
        let dispatch = concat!(
            include_str!("array_builtin_args.rs"),
            include_str!("mod.rs"),
            include_str!("../array_pointer_scan.rs"),
        );
        let variants: Vec<&str> = declared
            .split("pub enum BuiltinArgumentLowering {")
            .nth(1)
            .expect("the argument-lowering enum is declared in builtins::semantics")
            .split("\n}")
            .next()
            .expect("the enum body is brace-terminated")
            .lines()
            .map(str::trim)
            .filter(|line| {
                line.ends_with(',')
                    && !line.starts_with("///")
                    && line.chars().next().is_some_and(char::is_uppercase)
            })
            // A tuple variant carries a payload — `ArrayInternalPointer(ArrayPointerOp)` — and
            // only the identifier before the parenthesis is the name to look for.
            .map(|line| {
                let name = line.trim_end_matches(',');
                name.split_once('(').map_or(name, |(head, _)| head)
            })
            .filter(|name| *name != "Standard")
            .collect();
        assert!(
            variants.len() >= 8,
            "the enum parse found only {variants:?}, which cannot be the whole list"
        );
        // Comment lines are stripped first: a variant NAMED in prose is exactly the state this
        // test exists to reject, and the arm that ran `ReverseKeySort` was gone while the
        // module still talked about key sorts.
        let code: String = dispatch
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for variant in variants {
            assert!(
                code.contains(&format!("BuiltinArgumentLowering::{variant}")),
                "{variant} is declared and selectable but no arm in this file runs it"
            );
        }
    }
}
