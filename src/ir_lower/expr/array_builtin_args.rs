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

/// Lowers builtin call operands, applying builtin-specific preservation where source order matters.
pub(super) fn lower_builtin_call_args(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let rewritten = builtin_string_callbacks_as_first_class(sig, args);
    let args = rewritten.as_deref().unwrap_or(args);
    let operands = lower_planned_builtin_call_args(ctx, name, sig, args);
    let canonical = php_symbol_key(name.trim_start_matches('\\'));
    let lowering = crate::builtins::registry::lookup(&canonical)
        .map(|def| def.spec.semantics.argument_lowering);
    match lowering {
        Some(crate::builtins::semantics::BuiltinArgumentLowering::BareArrayValues { arg, sole }) => {
            lower_bare_array_values_operand(ctx, sig, args, operands, arg, sole)
        }
        _ => operands,
    }
}

/// Lowers a string literal naming a BUILTIN, at a `callable` parameter, as that builtin's
/// first-class callable: `array_filter($a, 'is_numeric')` as `array_filter($a, is_numeric(...))`.
///
/// The checker types a builtin named by string with the signature `is_numeric(...)` gets (see
/// `check_callback_builtin_call_in_engine_frame`). The lowering has to match it, because the
/// runtime resolves a callback STRING against user functions only: left as a string,
/// `array_filter` died at run time with "must be a valid callback", where `origin/main` had refused
/// the program at compile time — a regression, which this closes. In php the two spellings name the
/// same callable, so nothing observable changes; the first-class form is simply the one the backend
/// already builds a PHP-ABI wrapper for.
///
/// A builtin whose first-class form the backend cannot build yet (`usort` with `strcmp(...)`) is
/// then refused at compile time, loudly, instead of crashing at run time. Spreads are left alone:
/// which parameter a spread element lands on is not known here.
fn builtin_string_callbacks_as_first_class(
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    let sig = sig?;
    if args.iter().any(is_spread_arg) {
        return None;
    }
    // The builtin contracts type most callback parameters `mixed` — `array_filter`, `usort` and
    // `array_reduce` all do — so the type alone misses them. php-src names every one of them
    // `$callback`, and that convention is what identifies the position.
    let is_callable_param = |index: usize| {
        sig.params.get(index).is_some_and(|(name, ty)| {
            name.trim_start_matches('$') == "callback" || ty.codegen_repr() == PhpType::Callable
        })
    };
    let builtin_callable = |literal: &Expr| -> Option<Expr> {
        let ExprKind::StringLiteral(function) = &literal.kind else { return None };
        if !crate::name_resolver::is_builtin_function(function) {
            return None;
        }
        let key = php_symbol_key(function.trim_start_matches('\\'));
        if crate::builtins::registry::first_class_callable_rejection(&key).is_some()
            || crate::types::first_class_callable_builtin_sig(&key).is_none()
        {
            return None;
        }
        Some(Expr::new(
            ExprKind::FirstClassCallable(CallableTarget::Function(crate::names::Name::from(
                function.clone(),
            ))),
            literal.span,
        ))
    };
    let mut rewritten: Option<Vec<Expr>> = None;
    for (position, arg) in args.iter().enumerate() {
        let (index, literal) = match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                let Some(index) = sig.params.iter().position(|(param, _)| param == name) else {
                    continue;
                };
                (index, value.as_ref())
            }
            _ => (position, arg),
        };
        if !is_callable_param(index) {
            continue;
        }
        let Some(callable) = builtin_callable(literal) else { continue };
        let replacement = match &arg.kind {
            ExprKind::NamedArg { name, .. } => Expr::new(
                ExprKind::NamedArg { name: name.clone(), value: Box::new(callable) },
                arg.span,
            ),
            _ => callable,
        };
        rewritten.get_or_insert_with(|| args.to_vec())[position] = replacement;
    }
    rewritten
}

/// Replaces a bare-`array` operand with the owned `array<mixed>` list of its values.
///
/// See `BuiltinArgumentLowering::BareArrayValues` for why. Only an operand the checker typed as the
/// bare declaration — `array<mixed>|array<mixed, mixed>` — is converted: a typed array already
/// reaches the builtin's own path and needs nothing, and converting it would only copy it.
///
/// Ownership follows `coerce_operands_to_params`, which makes the same kind of substitution: every
/// by-value operand is rooted FIRST, because `array_values()` may throw and an unrooted owned
/// temporary evaluated earlier would then leak; the source is released if it was a temporary the
/// call owned, and the fresh list is rooted in its place.
fn lower_bare_array_values_operand(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    mut operands: Vec<crate::ir::ValueId>,
    arg: usize,
    sole: bool,
) -> Vec<crate::ir::ValueId> {
    if sole && (args.len() != 1 || args.iter().any(is_spread_arg)) {
        return operands;
    }
    let Some(&operand) = operands.get(arg) else {
        return operands;
    };
    if !ctx.builder.value_php_type(operand).is_php_array() {
        return operands;
    }
    let span = args.first().map_or_else(crate::span::Span::dummy, |first| first.span);
    for index in 0..operands.len() {
        if sig.is_some_and(|sig| sig.ref_params.get(index).copied().unwrap_or(false)) {
            continue;
        }
        let lowered = LoweredValue {
            value: operands[index],
            ir_type: ctx.builder.value_type(operands[index]),
        };
        operands[index] = root_evaluated_call_argument(ctx, lowered, span).value;
    }
    let source = LoweredValue {
        value: operands[arg],
        ir_type: ctx.builder.value_type(operands[arg]),
    };
    let values = ctx.emit_owned_value(
        crate::ir::Op::RuntimeCall,
        vec![source.value],
        Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::ArrayValues,
        ))),
        PhpType::Array(Box::new(PhpType::Mixed)),
        crate::ir_lower::effects_lookup::runtime_effects(),
        Some(span),
    );
    release_coerced_source_if_owned(ctx, source, Some(span));
    operands[arg] = root_evaluated_call_argument(ctx, values, span).value;
    operands
}

/// Lowers builtin operands through the builtin's own argument-lowering strategy.
fn lower_planned_builtin_call_args(
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
    let output_locals = prepare_write_only_output_locals(ctx, &canonical, sig, args);
    let capture_regex_output = (canonical == "preg_match" || canonical == "preg_match_all")
        && !output_locals.is_empty();
    if matches!(argument_lowering,
        crate::builtins::semantics::BuiltinArgumentLowering::Standard
        | crate::builtins::semantics::BuiltinArgumentLowering::MaterializeDefaults
        | crate::builtins::semantics::BuiltinArgumentLowering::PreserveValues
        | crate::builtins::semantics::BuiltinArgumentLowering::BareArrayValues { .. }
    ) {
        if let Some(sig) = sig {
            if let Some(operands) = dynamic_spreads::lower_boxed_spread_args(ctx, sig, args, name,
                argument_lowering == crate::builtins::semantics::BuiltinArgumentLowering::PreserveValues) {
                return operands;
            }
        }
    }
    if !crate::types::call_args::has_named_args(args)
        && !capture_regex_output
        && !matches!(argument_lowering,
            crate::builtins::semantics::BuiltinArgumentLowering::PcntlPreserveOmitted
            | crate::builtins::semantics::BuiltinArgumentLowering::PreserveValues)
    {
        if let Some(sig) = sig {
            if let Some(operands) = lower_positional_spread_args_with_signature(
                ctx, sig, args, Some(name), false, None,
            ) {
                for (name, ty) in output_locals {
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
            if capture_regex_output =>
        {
            // Publish an owner at the output's source-order evaluation point, not
            // while preparing storage before earlier arguments have run.
            lower_args_with_signature_options_for_capture(
                ctx, sig, args, true, false, Some((2, if canonical == "preg_match" {
                    PhpType::Array(Box::new(PhpType::Str))
                } else { PhpType::Array(Box::new(PhpType::Mixed)) })),
            )
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
            lower_positional_builtin_args_with_signature(ctx, sig, args)
        }
        _ => lower_args_with_signature(ctx, sig, args),
    };
    for (name, ty) in output_locals {
        let ty = if capture_regex_output {
            if ctx.local_type(&name).codegen_repr() == ty.codegen_repr() {
                ctx.local_type(&name)
            } else { PhpType::Mixed }
        } else { ty };
        ctx.set_local_logical_type(&name, ty);
    }
    lowered
}

/// Finds regex outputs without changing flow types, and prepares concrete PCNTL writeback slots.
fn prepare_write_only_output_locals(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<(String, PhpType)> {
    let mut outputs = Vec::new();
    if !crate::types::call_args::has_named_args(args) && !args.iter().any(is_spread_arg) {
        for (index, arg) in args.iter().enumerate() {
            if let Some(output) = prepare_write_only_output_local(ctx, canonical, index, arg) {
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
        if let Some(output) = prepare_write_only_output_local(ctx, canonical, index, expr) {
            outputs.push(output);
        }
    }
    outputs
}

/// Plans an output's frame representation without reinterpreting its pre-call value.
fn prepare_write_only_output_local(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    parameter_index: usize,
    value: &Expr,
) -> Option<(String, PhpType)> {
    let ty = match (canonical, parameter_index) {
        ("preg_match", 2) => PhpType::Array(Box::new(PhpType::Str)),
        ("preg_match_all", 2) => PhpType::Array(Box::new(PhpType::Mixed)),
        _ => pcntl_output_type(canonical, parameter_index)?,
    };
    let ExprKind::Variable(name) = &value.kind else {
        return None;
    };
    if (canonical == "preg_match" || canonical == "preg_match_all")
        && matches!(ctx.local_kinds.get(name),
            Some(crate::ir::LocalKind::StaticLocal | crate::ir::LocalKind::GlobalAlias))
    {
        // The regex backend diagnoses these unsupported output destinations itself.
        return None;
    }
    if canonical == "preg_match" || canonical == "preg_match_all" {
        // The shared argument planner promotes this slot when evaluating the
        // output argument. Earlier arguments must still see its original type.
        // Frame representation is whole-function metadata, not an evaluation:
        // expose it now so earlier narrowed loads receive their detached owner.
        if ctx.local_type(name).codegen_repr() != ty.codegen_repr() {
            if let Some(slot) = ctx.local_slots.get(name).copied() {
                ctx.builder.widen_local_storage_type(slot, PhpType::Mixed);
            }
        }
        return Some((name.clone(), ty));
    }
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
        ctx, Some(&storage), args, true, true,
        capture_output_index.map(|index| (index, PhpType::Mixed)),
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
/// change `$array[0]`. Converting the by-reference local to hash storage keeps each key/value pair
/// intact while allowing the runtime helper to reorder only the insertion-order links.
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
