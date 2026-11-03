//! Purpose:
//! Positional spread lowering and arity guards.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;
use crate::names::NameKind;

/// Lowers one trailing indexed spread in a fixed-arity positional call.
pub(super) fn lower_positional_spread_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
    builtin_name: Option<&str>,
    capture_values: bool,
    capture_output: Option<&(usize, PhpType)>,
) -> Option<Vec<crate::ir::ValueId>> {
    // Keep generic source planning intact. Only by-value indexed tails can be
    // materialized into one array without losing references or named keys.
    let projected = (!sig.ref_params.iter().any(|by_ref| *by_ref))
        .then(|| crate::types::call_args::coalesce_planned_indexed_spreads_with_prefix(
            args, if sig.variadic.is_some() {
                crate::types::call_args::regular_param_count(sig)
            } else { usize::MAX },
            |source| indexed_spread_source_type(ctx, source).is_some(),
        ))
        .flatten();
    let args = projected.as_deref().unwrap_or(args);
    let spread_idx = single_trailing_indexed_spread_arg(ctx, args)?;
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    if spread_idx > regular_param_count {
        return None;
    }
    let first_spread_param_idx = spread_idx;
    let required_len = required_positional_spread_len(sig, first_spread_param_idx, regular_param_count);
    let ExprKind::Spread(inner) = &args[spread_idx].kind else {
        return None;
    };
    if static_indexed_spread_len(inner).is_some_and(|len| len >= required_len) {
        return None;
    }

    // A variadic-only callee whose collector is `iterable` takes a keyed source as a HASH tail
    // (`collect(...$args)` forwarding a relay's own `...$args`). This path slices the source as an
    // indexed array, which reads a hash's buckets as slots, so the generic tail builder (it
    // spreads each source by its runtime shape) lowers that call instead.
    // An untyped collector is the same: `count_args(...$values)` over a declared `array` that
    // holds `[5 => 1, 9 => 2]`, or a list a by-reference unpack promoted to a hash, crashed here.
    if regular_param_count == 0
        && (super::variadic_args::variadic_param_is_iterable(sig)
            || variadic_param_is_untyped(sig))
        && !super::variadic_args::variadic_param_is_by_ref(sig)
        && indexed_spread_source_type(ctx, inner).is_some_and(|ty| match &ty {
            PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
            PhpType::Iterable | PhpType::Mixed | PhpType::Union(_) => true,
            _ => false,
        })
    {
        return None;
    }

    let mut operands = Vec::with_capacity(regular_param_count);
    for (index, arg) in args[..spread_idx].iter().enumerate() {
        let previous = capture_output.filter(|(output, _)| *output == index && !capture_values)
            .and_then(|(_, ty)| prepare_captured_output_argument(ctx, arg, ty));
        let value = lower_arg_with_signature_options(ctx, sig, index, arg, capture_values);
        if let Some((name, ty)) = previous { ctx.set_local_logical_type(&name, ty); }
        if capture_values || sig.ref_params.get(index).copied().unwrap_or(false) {
            operands.push(value);
        } else {
            let lowered = lowered_value_from_id(ctx, value);
            operands.push(root_evaluated_call_argument(ctx, lowered, arg.span).value);
        }
    }

    let source_ty = indexed_spread_source_type(ctx, inner)?;
    // A `callable` source is a DESCRIPTOR, not an array. Boxing it hands the element reads to the
    // gradual path, which rebuilds slot 0 (the bound receiver) and slot 1 (the bare method name)
    // from the descriptor. The two steps an array source needs are skipped deliberately:
    // `narrow_gradual_indexed_spread_source` would unbox it as runtime tag 4 and read a
    // descriptor as an array, and the min-length guard would call `ArrayLen` on it — a
    // callable array is exactly two elements by construction, so there is nothing to check.
    let callable_source = matches!(source_ty, PhpType::Callable);
    // An `iterable` is a bare list-or-hash pointer; boxing it records which, so the gradual
    // narrowing below takes it like any `mixed` source.
    let iterable_source = matches!(source_ty, PhpType::Iterable);
    // An `array<T>` with a concrete element type was inferred from a list and has integer keys.
    // `array<mixed>` is what a bare `array` DECLARATION types as, and that value may be a hash
    // at run time: `function f(array $args) { return new $c(...$args); }` handed a
    // `['b' => 'n']` bound nothing by name. So it takes the named probe like any gradual source.
    let may_carry_names = !matches!(
        &source_ty,
        PhpType::Array(element) if element.codegen_repr() != PhpType::Mixed
    );
    let spread = lower_expr(ctx, inner);
    // Box a `callable` source so the narrowing below sees a gradual value, then let that
    // narrowing MATERIALIZE it: `__rt_mixed_spread_array` turns a callable descriptor into a real
    // two-element array. Handing the boxed descriptor downstream instead left the hidden variadic
    // tail slicing a descriptor, and the callee's own `count()` then received null.
    // A source that may carry names AND is already a hash (a property or call declared to return
    // an associative array) is boxed too: the narrowing below lists a typed hash through
    // `array_values()`, which is right for a positional spread but threw away the string keys
    // the per-parameter name probes read. That is `ReflectionAttribute::newInstance()`'s
    // `new $this->__name(...$this->__args)` over a run-time materialized attribute: with
    // `#[Route('/p', name: 'n', requirements: [...])]` the values landed in declaration order
    // (`'n'` in the `requirements` array slot), a TypeError, and `debug:router` lost the route.
    let keyed_hash_source = may_carry_names
        && ctx.builder.value_type(spread.value) == IrType::Heap(IrHeapKind::Hash);
    let spread = if callable_source || iterable_source || keyed_hash_source {
        ctx.box_value_as_mixed(spread, PhpType::Mixed, Some(args[spread_idx].span))
    } else {
        spread
    };
    let spread = narrow_gradual_indexed_spread_source(ctx, spread, args[spread_idx].span, None);
    let spread = root_evaluated_call_argument(ctx, spread, inner.span);
    let spread_type = ctx.builder.value_php_type(spread.value);
    let temp_name = ctx.declare_hidden_temp(spread_type.clone());
    store_value_into_temp(ctx, &temp_name, spread_type, spread, args[spread_idx].span);
    let spread_expr = Expr::new(ExprKind::Variable(temp_name), inner.span);
    let spread_value = lower_expr(ctx, &spread_expr);
    if let Some(name) = builtin_name {
        emit_builtin_spread_arity_guard(
            ctx, spread_value.value, required_len,
            sig.variadic.is_none().then_some(regular_param_count - spread_idx),
            name, args[spread_idx].span,
        );
    } else {
        emit_positional_spread_min_len_guard(
            ctx, spread_value.value, required_len, args[spread_idx].span,
        );
    }

    for param_idx in first_spread_param_idx..regular_param_count {
        let element_idx = param_idx - first_spread_param_idx;
        let default = sig.defaults.get(param_idx).and_then(|default| default.as_ref());
        // Unpacking is decided PER KEY at runtime: a string key names a parameter, an integer key
        // fills the next position, in the same array. This module is chosen whenever the source's
        // type is `array`/`mixed` -- which a function declared `: array` satisfies whatever keys
        // it returns -- so reading positionally ONLY dropped every name the source carried.
        //
        // MEASURED: `new Target(...namedArgs())`, where `namedArgs(): array` returns
        // `['name' => 'fromCall', 'description' => 'desc']`, left both parameters at their
        // defaults while php bound both. That is `ReflectionAttribute::newInstance()`'s
        // `new $this->__name(...$this->__args)`, so every `#[AsCommand(name: 'x')]` was
        // constructed empty and Symfony's console reported
        // `AsCommand::__construct(): Argument #1 ($name) must be of type string, array given`.
        //
        // The element builders ask for the name first and fall back to the position, so a packed
        // list still reads exactly as it did.
        // Only pay for the named probe when the source COULD carry names. A packed `array<T>`
        // cannot, and asking anyway added an `array_key_exists` call and a ternary to every spread
        // parameter in the program -- 26 MB of extra assembly on the Symfony build.
        let param_name = may_carry_names
            .then(|| sig.params.get(param_idx).map(|(name, _)| name.as_str()))
            .flatten();
        let expr = if let Some(default) = default {
            if element_idx < required_len {
                spread_element_expr_for_ir(
                    &spread_expr,
                    element_idx,
                    param_name,
                    may_carry_names,
                    args[spread_idx].span,
                )
            } else {
                spread_element_or_default_expr_for_ir(
                    &spread_expr,
                    element_idx,
                    param_name,
                    may_carry_names,
                    default.clone(),
                    args[spread_idx].span,
                )
            }
        } else {
            spread_element_expr_for_ir(
                &spread_expr,
                element_idx,
                param_name,
                may_carry_names,
                args[spread_idx].span,
            )
        };
        operands.push(lower_arg_with_signature_options(ctx, sig, param_idx, &expr, capture_values));
    }

    if sig.variadic.is_some() {
        let tail_offset = regular_param_count.saturating_sub(spread_idx);
        let actual_count = positional_spread_actual_count_expr(
            &spread_expr,
            spread_idx,
            args[spread_idx].span,
        );
        if crate::func_args::sig_has_hidden_argc_param(sig) {
            operands.push(lower_expr(ctx, &actual_count).value);
        }
        let tail = if crate::func_args::sig_collects_optional_arg_count(sig) {
            let tail = positional_spread_tail_expr(
                &spread_expr,
                tail_offset,
                args[spread_idx].span,
            );
            let tail = Expr::new(
                ExprKind::ArrayLiteral(vec![
                    actual_count,
                    Expr::new(ExprKind::Spread(Box::new(tail)), args[spread_idx].span),
                ]),
                args[spread_idx].span,
            );
            lower_expr(ctx, &tail)
        } else {
            lower_positional_spread_tail(
                ctx,
                sig,
                &spread_expr,
                tail_offset,
                args[spread_idx].span,
            )
        };
        let tail = coerce_spread_variadic_array(ctx, sig, tail, args[spread_idx].span);
        operands.push(tail.value);
    }

    Some(coerce_operands_to_params(ctx, sig, operands))
}

/// Whether the variadic collector takes any value (`...$xs`, `mixed ...$xs`), so no element
/// coercion stands between the source and the generic tail builder.
fn variadic_param_is_untyped(sig: &FunctionSig) -> bool {
    crate::types::call_args::variadic_element_type(sig)
        .is_some_and(|element| element.codegen_repr() == PhpType::Mixed)
}

/// Counts a staged unpack using its actual storage representation, not its PHP array hint.
pub(super) fn lower_spread_length(
    ctx: &mut LoweringContext<'_, '_>,
    spread: ValueId,
    span: Span,
) -> LoweredValue {
    if matches!(ctx.builder.value_php_type(spread).codegen_repr(), PhpType::Array(_)) {
        return ctx.emit_value(
            Op::ArrayLen, vec![spread], None, PhpType::Int,
            Op::ArrayLen.default_effects(), Some(span),
        );
    }
    let target = crate::ir::RuntimeFnId::Count;
    ctx.emit_value(
        Op::RuntimeCall,
        vec![spread],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))),
        PhpType::Int,
        target.effects(),
        Some(span),
    )
}

/// Rejects runtime builtin arity violations without discarding surplus unpacked values.
fn emit_builtin_spread_arity_guard(
    ctx: &mut LoweringContext<'_, '_>,
    spread: ValueId,
    min: usize,
    max: Option<usize>,
    name: &str,
    span: Span,
) {
    let len = lower_spread_length(ctx, spread, span);
    for (bound, predicate, direction) in [
        (Some(min), CmpPredicate::Sge, "few"),
        (max, CmpPredicate::Sle, "many"),
    ] {
        let Some(bound) = bound else { continue };
        let bound = emit_i64_at_span(ctx, bound as i64, span);
        let valid = ctx.emit_value(
            Op::ICmp, vec![len.value, bound.value],
            Some(Immediate::CmpPredicate(predicate)), PhpType::Bool,
            Op::ICmp.default_effects(), Some(span),
        );
        let ok = ctx.builder.create_named_block("builtin.spread.arity.ok", Vec::new());
        let invalid = ctx.builder.create_named_block("builtin.spread.arity.invalid", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: valid.value, then_target: ok, then_args: Vec::new(),
            else_target: invalid, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(invalid);
        let exception = Expr::new(ExprKind::NewObject {
            class_name: Name::unqualified("ArgumentCountError"),
            args: vec![Expr::new(ExprKind::StringLiteral(
                format!("{name}(): Too {direction} arguments for unpacked call")
            ), span)],
        }, span);
        let exception = lower_expr(ctx, &exception);
        ctx.builder.terminate(Terminator::Throw { value: exception.value });
        ctx.builder.position_at_end(ok);
    }
}

/// Lowers a compiler-generated spread tail with the variadic slot's concrete element layout.
fn lower_positional_spread_tail(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    spread: &Expr,
    offset: usize,
    span: crate::span::Span,
) -> LoweredValue {
    let source = lower_expr(ctx, spread);
    let offset = emit_i64_at_span(ctx, offset as i64, span);
    let target = crate::ir::RuntimeFnId::ArraySlice;
    // The slice copies the SOURCE's slot layout. A gradual source (a declared `: array` return,
    // `array<mixed>` after the merge) holds boxed slots that the slice cannot narrow into a typed
    // variadic's raw ones, so it keeps the source layout and the parameter coercion converts it.
    let expected = variadic_array_type(sig);
    let source_ty = ctx.builder.value_php_type(source.value).codegen_repr();
    let result_ty = match (&source_ty, expected.codegen_repr()) {
        (PhpType::Array(source_elem), PhpType::Array(expected_elem))
            if source_elem.codegen_repr() == PhpType::Mixed
                && expected_elem.codegen_repr() != PhpType::Mixed =>
        {
            source_ty.clone()
        }
        _ => expected,
    };
    ctx.emit_value(
        Op::RuntimeCall,
        vec![source.value, offset.value],
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::Function(target),
        )),
        result_ty,
        target.effects(),
        Some(span),
    )
}

/// Builds `array_slice($spread, $offset)` for the values beyond regular parameters.
fn positional_spread_tail_expr(spread: &Expr, offset: usize, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(NameKind::FullyQualified, vec!["array_slice".to_string()]),
            args: vec![
                spread.clone(),
                Expr::new(ExprKind::IntLiteral(offset as i64), span),
            ],
        },
        span,
    )
}

/// Builds the runtime count of explicit prefix values plus one dynamic indexed spread.
fn positional_spread_actual_count_expr(
    spread: &Expr,
    prefix_len: usize,
    span: crate::span::Span,
) -> Expr {
    let spread_count = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(NameKind::FullyQualified, vec!["count".to_string()]),
            args: vec![spread.clone()],
        },
        span,
    );
    if prefix_len == 0 {
        spread_count
    } else {
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(ExprKind::IntLiteral(prefix_len as i64), span)),
                op: BinOp::Add,
                right: Box::new(spread_count),
            },
            span,
        )
    }
}

/// Converts a sliced dynamic tail to the variadic slot's indexed storage representation.
fn coerce_spread_variadic_array(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    tail: LoweredValue,
    span: crate::span::Span,
) -> LoweredValue {
    let expected = variadic_array_type(sig);
    let actual = ctx.builder.value_php_type(tail.value).codegen_repr();
    let needs_mixed = matches!(expected.codegen_repr(), PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed)
        && !matches!(actual, PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed);
    if !needs_mixed {
        return tail;
    }
    ctx.emit_value(
        Op::ArrayToMixed,
        vec![tail.value],
        None,
        expected,
        Op::ArrayToMixed.default_effects(),
        Some(span),
    )
}

/// Returns an indexed spread length only when no nested unpack can change its size.
pub(super) fn static_indexed_spread_len(expr: &Expr) -> Option<usize> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) if !items.iter().any(is_spread_arg) => Some(items.len()),
        _ => None,
    }
}

/// Returns the index of a single trailing positional spread that EIR can materialize.
pub(super) fn single_trailing_indexed_spread_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<usize> {
    let spread_indices = args
        .iter()
        .enumerate()
        .filter_map(|(idx, arg)| matches!(arg.kind, ExprKind::Spread(_)).then_some(idx))
        .collect::<Vec<_>>();
    let [spread_idx] = spread_indices.as_slice() else {
        return None;
    };
    if *spread_idx + 1 != args.len() {
        return None;
    }
    let ExprKind::Spread(inner) = &args[*spread_idx].kind else {
        return None;
    };
    indexed_spread_source_type(ctx, inner)?;
    Some(*spread_idx)
}

/// Returns the indexed-array source type for spread-only EIR lowering.
pub(super) fn indexed_spread_source_type(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<PhpType> {
    let ty = match &expr.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, expr),
        // Unpacking consumes the getter's native container, rather than an ordinary
        // literal element whose logical type must still include a missing read's null.
        ExprKind::ArrayAccess { .. } | ExprKind::ErrorSuppress(_) => {
            spread_source_storage_type_for_ir(ctx, expr)
        }
        // A property read is GRADUAL for this decision. The syntactic fallback below cannot see a
        // declared property type, so `...$this->controller` used to leave the spread path
        // entirely and lower as a single operand -- a hard `OperandCountMismatch` at compile
        // time. Treating it as gradual routes it through `__rt_mixed_spread_array`, which passes
        // a real array through, rebuilds a callable descriptor's two elements, and answers null
        // for anything else so the length guard raises PHP's own unpack error.
        ExprKind::PropertyAccess { .. }
        | ExprKind::NullsafePropertyAccess { .. }
        | ExprKind::DynamicPropertyAccess { .. } => PhpType::Mixed,
        _ => array_literal_element_type_for_ir(ctx, expr),
    }
    .codegen_repr();
    // `Callable` joins the gradual shapes: its slot holds a descriptor, and a descriptor built
    // from PHP's `[$object, 'method']` syntax rebuilds both elements on demand (see
    // `lower_mixed_callable_slot_or_array_get`). The caller boxes it so those reads take the
    // gradual path.
    // `Iterable` too: a variadic parameter that received a named argument is typed so (its
    // storage may be a list or a hash). Left out, `new $class(...$args)` over it took the generic
    // runtime-container path, which bound no named key and returned no object.
    if matches!(
        ty,
        PhpType::Array(_)
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Callable
            | PhpType::Iterable
    ) {
        Some(ty)
    } else {
        None
    }
}

/// Returns how many spread elements must exist to satisfy required parameters.
pub(super) fn required_positional_spread_len(
    sig: &FunctionSig,
    start_param_idx: usize,
    regular_param_count: usize,
) -> usize {
    (start_param_idx..regular_param_count)
        .rfind(|idx| sig.defaults.get(*idx).and_then(|default| default.as_ref()).is_none())
        .map(|idx| idx - start_param_idx + 1)
        .unwrap_or(0)
}

/// Emits a fatal guard when a positional spread is shorter than required parameters.
pub(super) fn emit_positional_spread_min_len_guard(
    ctx: &mut LoweringContext<'_, '_>,
    spread: crate::ir::ValueId,
    min_len: usize,
    span: crate::span::Span,
) {
    if min_len == 0 {
        return;
    }
    let len = lower_spread_length(ctx, spread, span);
    let min = emit_i64_at_span(ctx, min_len as i64, span);
    let has_required_args = ctx.emit_value(
        Op::ICmp,
        vec![len.value, min.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sge)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let ok = ctx.builder.create_named_block("call.spread.len.ok", Vec::new());
    let fatal = ctx.builder.create_named_block("call.spread.len.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_required_args.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: fatal,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    let message = ctx.intern_string("Fatal error: too few arguments for spread call\n");
    ctx.builder.terminate(Terminator::Fatal { message });

    ctx.builder.position_at_end(ok);
}
