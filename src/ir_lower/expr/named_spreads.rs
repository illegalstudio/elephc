//! Purpose:
//! Named spread bounds, associative reads, and variadic-tail setup.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns a static prefix length only for indexed array literals without nested spreads.
pub(super) fn static_indexed_variadic_prefix_len(prefix_expr: &Expr) -> Option<usize> {
    let ExprKind::ArrayLiteral(items) = &prefix_expr.kind else {
        return None;
    };
    if items.iter().any(|item| matches!(item.kind, ExprKind::Spread(_))) {
        return None;
    }
    Some(items.len())
}

/// Builds a variadic tail hash from static spread overflow plus later named variadics.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_named_spread_static_variadic_tail_hash(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    prefix_temp: &Expr,
    prefix_len: usize,
    regular_param_count: usize,
    plan: &crate::types::call_args::CallArgPlan,
    source_values: &[Option<crate::ir::ValueId>],
    first_named_pos: usize,
    span: crate::span::Span,
) -> LoweredValue {
    let value_ty = variadic_tail_value_type(sig);
    let prefix_tail_len = prefix_len.saturating_sub(regular_param_count);
    let named_tail_len = plan
        .source_values
        .iter()
        .filter(|source| source.source_index() >= first_named_pos && source.param_idx().is_none())
        .count();
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(value_ty.clone()),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity((prefix_tail_len + named_tail_len) as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(span),
    );
    let array_ty = PhpType::Array(Box::new(value_ty.clone()));
    let mut next_positional_key = 0usize;
    for prefix_idx in regular_param_count..prefix_len {
        let key = emit_i64_at_span(ctx, next_positional_key as i64, span);
        next_positional_key += 1;
        let expr = spread_element_expr_for_ir(prefix_temp, prefix_idx, None, false, span);
        let value = lower_expr(ctx, &expr);
        let value = coerce_variadic_tail_value(ctx, value, &array_ty, span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(span),
        );
    }
    for source in &plan.source_values {
        if source.source_index() < first_named_pos || source.param_idx().is_some() {
            continue;
        }
        let key = if let Some(key) = source.key() {
            lower_string_literal(ctx, key, source.expr())
        } else {
            let key = emit_i64_at_span(ctx, next_positional_key as i64, source.expr().span);
            next_positional_key += 1;
            key
        };
        let value = source_values[source.source_index()]
            .expect("named spread variadic source was not evaluated");
        let value = lowered_value_from_id(ctx, value);
        let value = coerce_variadic_tail_value(ctx, value, &array_ty, source.expr().span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(source.expr().span),
        );
    }
    hash
}

/// Emits named-after-spread min/max checks against the already materialized prefix temp.
pub(super) fn emit_named_spread_bounds_guard(
    ctx: &mut LoweringContext<'_, '_>,
    spread: crate::ir::ValueId,
    check: &crate::types::call_args::SpreadBoundsCheck,
    span: crate::span::Span,
) {
    if check.min_len == 0 && check.max_len.is_none() {
        return;
    }
    let len = lower_spread_length(ctx, spread, span);
    emit_named_spread_min_len_guard(ctx, len.value, check.min_len, span);
    emit_named_spread_max_len_guard(
        ctx,
        len.value,
        check.max_len,
        check.max_len_param_name.as_deref(),
        span,
    );
}

/// Emits the underflow branch for a named-after-spread bounds check.
pub(super) fn emit_named_spread_min_len_guard(
    ctx: &mut LoweringContext<'_, '_>,
    len: crate::ir::ValueId,
    min_len: usize,
    span: crate::span::Span,
) {
    if min_len == 0 {
        return;
    }
    let min = emit_i64_at_span(ctx, min_len as i64, span);
    let has_required_args = ctx.emit_value(
        Op::ICmp,
        vec![len, min.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sge)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let ok = ctx.builder.create_named_block("call.named_spread.min.ok", Vec::new());
    let fatal = ctx.builder.create_named_block("call.named_spread.min.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_required_args.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: fatal,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    let message = ctx.intern_string("Fatal error: named argument spread length mismatch\n");
    ctx.builder.terminate(Terminator::Fatal { message });

    ctx.builder.position_at_end(ok);
}

/// Emits the overflow branch for a named-after-spread bounds check.
pub(super) fn emit_named_spread_max_len_guard(
    ctx: &mut LoweringContext<'_, '_>,
    len: crate::ir::ValueId,
    max_len: Option<usize>,
    param_name: Option<&str>,
    span: crate::span::Span,
) {
    let Some(max_len) = max_len else {
        return;
    };
    let max = emit_i64_at_span(ctx, max_len as i64, span);
    let within_bound = ctx.emit_value(
        Op::ICmp,
        vec![len, max.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sle)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let ok = ctx.builder.create_named_block("call.named_spread.max.ok", Vec::new());
    let fatal = ctx.builder.create_named_block("call.named_spread.max.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: within_bound.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: fatal,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    if let Some(param_name) = param_name {
        super::named_args::throw_named_parameter_overwrite(ctx, param_name, span);
    } else {
        let message = ctx.intern_string("Fatal error: named argument spread length mismatch\n");
        ctx.builder.terminate(Terminator::Fatal { message });
    }

    ctx.builder.position_at_end(ok);
}

/// Lowers a single associative spread as named parameter reads by key.
///
/// The hidden `func_get_args()` collector `debug_backtrace()` arms on every body is no source
/// variadic: the reads cover the source-visible parameters and the collector is appended, so
/// Monolog's `new self(...$args)` keeps compiling. A body with optional parameters reads the
/// passed-argument count first; it is the position after the last parameter the array binds.
pub(super) fn lower_assoc_spread_only_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<Vec<crate::ir::ValueId>> {
    let [arg] = args else {
        return None;
    };
    let ExprKind::Spread(inner) = &arg.kind else {
        return None;
    };
    let hidden_collector = sig.variadic.as_deref() == Some(crate::func_args::HIDDEN_ARGS_PARAM)
        && !crate::func_args::sig_has_hidden_argc_param(sig);
    if !is_assoc_spread_source(ctx, inner) || (sig.variadic.is_some() && !hidden_collector) {
        return None;
    }
    let source_params = if hidden_collector {
        sig.params.len().checked_sub(1)?
    } else {
        sig.params.len()
    };
    let spread = lower_expr(ctx, inner);
    let spread = root_evaluated_call_argument(ctx, spread, inner.span);
    let spread_type = ctx.builder.value_php_type(spread.value);
    let temp_name = ctx.declare_hidden_temp(spread_type.clone());
    store_value_into_temp(ctx, &temp_name, spread_type, spread, arg.span);
    let spread_expr = Expr::new(ExprKind::Variable(temp_name), inner.span);
    let mut operands = Vec::with_capacity(sig.params.len());
    for (idx, (param_name, _)) in sig.params.iter().enumerate().take(source_params) {
        let default = sig.defaults.get(idx).and_then(|default| default.as_ref());
        let param_expr =
            assoc_spread_param_expr(&spread_expr, idx, param_name, default, arg.span);
        operands.push(lower_arg_with_signature(ctx, sig, idx, &param_expr));
    }
    if hidden_collector {
        let span = arg.span;
        let elements = if crate::func_args::sig_collects_optional_arg_count(sig) {
            let key_exists = |key: Expr| {
                Expr::new(
                    ExprKind::FunctionCall {
                        name: Name::unqualified("array_key_exists"),
                        args: vec![key, spread_expr.clone()],
                    },
                    span,
                )
            };
            let mut count = Expr::new(ExprKind::IntLiteral(0), span);
            for (idx, (param_name, _)) in sig.params.iter().enumerate().take(source_params) {
                let bound = Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(key_exists(Expr::new(
                            ExprKind::StringLiteral(param_name.clone()),
                            span,
                        ))),
                        op: crate::parser::ast::BinOp::Or,
                        right: Box::new(key_exists(Expr::new(
                            ExprKind::IntLiteral(idx as i64),
                            span,
                        ))),
                    },
                    span,
                );
                count = Expr::new(
                    ExprKind::Ternary {
                        condition: Box::new(bound),
                        then_expr: Box::new(Expr::new(ExprKind::IntLiteral(idx as i64 + 1), span)),
                        else_expr: Box::new(count),
                    },
                    span,
                );
            }
            vec![count]
        } else {
            Vec::new()
        };
        let collector = Expr::new(ExprKind::ArrayLiteral(elements), span);
        operands.push(lower_arg_with_signature(ctx, sig, source_params, &collector));
    }
    Some(operands)
}

/// Builds an expression that reads one parameter from a spread whose array type has keys that
/// may be strings: the parameter's name, else its position, else its default.
///
/// PHP binds `f(...$a)` per KEY: `$a[0]` goes to the first parameter and `$a['b']` to `$b`, in
/// the same unpack. This read the name only, so a plain `array $args` spread with a list --
/// `f(...[1, 2])` held in a variable -- bound every parameter to its default.
pub(super) fn assoc_spread_param_expr(
    spread_expr: &Expr,
    position: usize,
    param_name: &str,
    default: Option<&Expr>,
    span: crate::span::Span,
) -> Expr {
    let read = |key: Expr| {
        Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(spread_expr.clone()),
                index: Box::new(key),
            },
            span,
        )
    };
    let key_exists = |key: Expr| {
        Box::new(Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("array_key_exists"),
                args: vec![key, spread_expr.clone()],
            },
            span,
        ))
    };
    let name = Expr::new(ExprKind::StringLiteral(param_name.to_string()), span);
    let index = Expr::new(ExprKind::IntLiteral(position as i64), span);
    let by_position = match default {
        Some(default) => Expr::new(
            ExprKind::Ternary {
                condition: key_exists(index.clone()),
                then_expr: Box::new(read(index)),
                else_expr: Box::new(default.clone()),
            },
            span,
        ),
        None => read(index),
    };
    Expr::new(
        ExprKind::Ternary {
            condition: key_exists(name.clone()),
            then_expr: Box::new(read(name)),
            else_expr: Box::new(by_position),
        },
        span,
    )
}

/// Builds an expression that reads one materialized spread element from a hidden temp.
///
/// PHP decides per KEY, not per array: `f(...$a)` binds `$a['name']` to the parameter named
/// `name` and `$a[0]` to the first position, in the SAME unpack. This used to pick one or the
/// other from `prefer_named_key`, so a source carrying names but classified positional (or the
/// reverse) silently read the wrong slot. The hybrid asks for the name first and falls back to
/// the position, which is right for a keyed array, a packed list, and the mixed form alike.
pub(super) fn spread_element_expr_for_ir(
    spread_expr: &Expr,
    element_idx: usize,
    param_name: Option<&str>,
    prefer_named_key: bool,
    span: crate::span::Span,
) -> Expr {
    let positional = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(spread_expr.clone()),
            index: Box::new(Expr::new(ExprKind::IntLiteral(element_idx as i64), span)),
        },
        span,
    );
    let Some(param_name) = param_name.filter(|_| prefer_named_key) else {
        return positional;
    };
    let key = Expr::new(ExprKind::StringLiteral(param_name.to_string()), span);
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("array_key_exists"),
                    args: vec![key.clone(), spread_expr.clone()],
                },
                span,
            )),
            then_expr: Box::new(Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(spread_expr.clone()),
                    index: Box::new(key),
                },
                span,
            )),
            else_expr: Box::new(positional),
        },
        span,
    )
}

/// Builds an expression that falls back to a default when a spread element is absent.
pub(super) fn spread_element_or_default_expr_for_ir(
    spread_expr: &Expr,
    element_idx: usize,
    param_name: Option<&str>,
    prefer_named_key: bool,
    default_expr: Expr,
    span: crate::span::Span,
) -> Expr {
    // The slot is present under EITHER spelling, so the two are asked in PHP's own order: the
    // NAME first, then the position, then the parameter's default. Written as one flat condition
    // plus a second probe inside the value it cost THREE `array_key_exists` calls per optional
    // parameter; nested, the same answer costs two. That matters: every spread parameter in the
    // program pays it, and the flat form added 22% to the Symfony `--web` compile's CPU time.
    let positional = Expr::new(
        ExprKind::Ternary {
            condition: Box::new(spread_len_gt_expr_for_ir(spread_expr, element_idx, span)),
            then_expr: Box::new(spread_element_expr_for_ir(
                spread_expr,
                element_idx,
                None,
                false,
                span,
            )),
            else_expr: Box::new(default_expr),
        },
        span,
    );
    let Some(param_name) = param_name.filter(|_| prefer_named_key) else {
        return positional;
    };
    let key = Expr::new(ExprKind::StringLiteral(param_name.to_string()), span);
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("array_key_exists"),
                    args: vec![key.clone(), spread_expr.clone()],
                },
                span,
            )),
            then_expr: Box::new(Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(spread_expr.clone()),
                    index: Box::new(key),
                },
                span,
            )),
            else_expr: Box::new(positional),
        },
        span,
    )
}

/// Builds `array_key_exists(element_idx, $spread)` for optional spread-slot defaults.
///
/// This guard used to be `count($spread) > element_idx`, which is only equivalent for a PACKED
/// list. Since PHP 8.1 an unpacked array may carry STRING keys — they name parameters — and it
/// may be sparse, and then the length says nothing about whether slot `element_idx` is there.
/// `f(...['a', 'name' => 'x', 'methods' => ['GET']])` has `count() === 3`, so the old guard took
/// the read arm for slots 1 and 2, read two keys that do not exist, and raised
/// `Undefined array key 1` / `... 2` before handing the parameter a null instead of its default.
///
/// That is not a cosmetic difference. Every diagnostic goes through `__elephc_diag_dispatch`,
/// and a program with a user error handler installed (`set_error_handler`) pays a full
/// `call_user_func_array` dispatch for each one. Measured on the compiled Symfony `--web`
/// fixture, where `ReflectionAttribute::newInstance()` unpacks exactly such an
/// attribute-argument array: **27.3% of the whole request** was spent raising and dispatching
/// four warnings per request that `php -S` does not raise at all.
///
/// Asking whether the key is there is both the cheaper question and the correct one: a slot that
/// is absent takes the parameter's own default, which is what PHP passes.
pub(super) fn spread_len_gt_expr_for_ir(
    spread_expr: &Expr,
    element_idx: usize,
    span: crate::span::Span,
) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("array_key_exists"),
            args: vec![
                Expr::new(ExprKind::IntLiteral(element_idx as i64), span),
                spread_expr.clone(),
            ],
        },
        span,
    )
}

/// Marks spread arguments whose source is known to be an associative array.
pub(super) fn assoc_spread_sources(ctx: &LoweringContext<'_, '_>, args: &[Expr]) -> Vec<bool> {
    crate::types::call_args::expand_static_assoc_spread_args(args)
        .iter()
        .map(|arg| match &arg.kind {
            ExprKind::Spread(inner) => is_assoc_spread_source(ctx, inner),
            _ => false,
        })
        .collect()
}

/// Returns true when a spread expression may feed named parameters by key.
///
/// The question is NOT "is this an associative array" -- PHP answers that per key, at runtime --
/// but "can this source be ruled out as a packed list". Only a positional array literal, or a
/// local the checker typed as a packed `array`, can be. Everything else (a call, a property, a
/// parameter, a `mixed`) takes the hybrid read.
///
/// Requiring a statically-known `AssocArray` is what dropped the names whenever the source was a
/// CALL or a PROPERTY: `new $class(...$this->__args)` -- which is
/// `ReflectionAttribute::newInstance()`, and how every `#[AsCommand(name: 'x')]` is constructed --
/// bound nothing and left every parameter at its default, so Symfony's console reported
/// `AsCommand::__construct(): Argument #1 ($name) must be of type string, array given`.
pub(super) fn is_assoc_spread_source(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    // `PhpType::Array(_)` is NOT a proof: a function declared `: array` returning
    // `['name' => 'x']` carries that type, and ruling its result out is what kept
    // `new $class(...namedArgs())` from binding anything. A POSITIONAL ARRAY LITERAL written at
    // the call site is the only source whose keys are known here.
    //
    // A source typed `array<T>` with a concrete element type was inferred from a list and has
    // integer keys, so it is ruled out as well: treating `...$pos` in `f(...$pos, ...$args, b: 1)`
    // as a name provider made the FIRST provider answer for every slot (`$pos[2]`, undefined), and
    // `f(...args(), c: 1)` evaluated its call once per slot. `array<mixed>` is what a bare `array`
    // declaration types as and may hold names, so it still probes.
    if matches!(expr.kind, ExprKind::ArrayLiteral(_)) {
        return false;
    }
    !matches!(
        super::positional_spreads::indexed_spread_source_type(ctx, expr),
        Some(PhpType::Array(element)) if element.codegen_repr() != PhpType::Mixed
    )
}
