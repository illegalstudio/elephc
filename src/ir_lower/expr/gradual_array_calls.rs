//! Purpose:
//! Routes the array builtins whose operands or flags are only known at run time to the helpers of
//! `crate::array_gradual_prelude`.
//!
//! Called from:
//! - `crate::ir_lower::expr::function_calls::lower_function_call()` before registry lowering.
//!
//! Key details:
//! - The native array runtimes walk one statically known layout; php's functions accept whatever
//!   the value holds and check it when the call runs. Each arm here is the lowering half of a
//!   checker decision: the checker files `php_array()` (or `mixed`) for exactly the operand
//!   shapes it could not give a layout, and the arm redirects on that FILED type, so the two
//!   cannot disagree about which calls the helper answers.
//! - `sort()`/`rsort()`/`asort()`/`arsort()` with a `$flags` that is not the literal
//!   `SORT_REGULAR` become `usort()`/`uasort()` with the comparator the flags select; the native
//!   sorts implement only the default comparison. Reversed sorts swap the comparator's operands,
//!   which keeps equal elements in their original order as php's reverse comparators do.
//! - `array_merge_recursive()` with other than two arrays folds into two-array calls (native) or,
//!   for a spread, into the prelude loop.

use super::*;
use crate::array_gradual_prelude as prelude;
use crate::parser::ast::ArrayEntry;
use crate::synthetic_class::{e_bool, e_call};

/// Routes a gradual-array builtin call to its helper, or `None` when the native lowering applies.
pub(super) fn lower_gradual_array_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let builtin = php_symbol_key(name.trim_start_matches('\\'));
    if let Some(value) = lower_spread_call(ctx, &builtin, args, expr) {
        return Some(value);
    }
    if args.iter().any(is_spread_arg) {
        return None;
    }
    match builtin.as_str() {
        "sort" | "rsort" | "asort" | "arsort" => lower_flagged_sort(ctx, &builtin, name, args, expr),
        "array_unique" => lower_array_unique(ctx, name, args, expr),
        "array_merge_recursive" => lower_array_merge_recursive(ctx, args, expr),
        "array_diff_assoc" | "array_intersect_assoc" => {
            let helper = if builtin == "array_diff_assoc" {
                prelude::ARRAY_DIFF_ASSOC_NAME
            } else {
                prelude::ARRAY_INTERSECT_ASSOC_NAME
            };
            (args.len() == 2 && filed_type(ctx, expr).is_some_and(PhpType::is_php_array))
                .then(|| call_helper(ctx, helper, args, expr))
        }
        "array_pad" => (args.len() == 3 && filed_type(ctx, expr).is_some_and(PhpType::is_php_array))
            .then(|| call_helper(ctx, prelude::ARRAY_PAD_NAME, args, expr)),
        "array_reverse" => lower_array_reverse(ctx, args, expr),
        "iterator_to_array" => lower_scalar_iterator_source(ctx, args, expr),
        "pcntl_sigprocmask" | "pcntl_sigwaitinfo" | "pcntl_sigtimedwait" => {
            lower_gradual_signal_set(ctx, &builtin, name, args, expr)
        }
        _ => None,
    }
}

/// Returns the type the checker filed for this builtin call, when it filed one.
fn filed_type<'a>(ctx: &'a LoweringContext<'_, '_>, expr: &Expr) -> Option<&'a PhpType> {
    if expr.span.line == 0 {
        return None;
    }
    ctx.builtin_call_types
        .get(&(ctx.loop_storage_scope.clone(), expr.span))
}

/// Lowers a call to one of the prelude helpers.
fn call_helper(
    ctx: &mut LoweringContext<'_, '_>,
    helper: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    lower_function_call(ctx, &crate::names::Name::unqualified(helper), args, expr)
}

/// Looks through a `name:` wrapper to the written value.
fn written_value(arg: &Expr) -> &Expr {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => value,
        _ => arg,
    }
}

/// Returns the arguments in positional order when every one is positional or named with one of
/// `names`, filling none: `sort($a, flags: 2)` reads like `sort($a, 2)`. `None` for any other
/// named argument, which keeps the native binder.
fn positional_values<'a>(args: &'a [Expr], names: &[&str]) -> Option<Vec<&'a Expr>> {
    let mut values = vec![None; names.len()];
    for (index, arg) in args.iter().enumerate() {
        let slot = match &arg.kind {
            ExprKind::NamedArg { name, .. } => names.iter().position(|known| *known == name)?,
            _ => index,
        };
        if slot >= values.len() || values[slot].is_some() {
            return None;
        }
        values[slot] = Some(written_value(arg));
    }
    let present = values.iter().take_while(|value| value.is_some()).count();
    if values[present..].iter().any(Option::is_some) {
        return None;
    }
    Some(values.into_iter().flatten().collect())
}

/// Builds a call expression carrying the original call's position.
fn synthetic_call(name: &str, args: Vec<Expr>, expr: &Expr) -> Expr {
    let mut call = e_call(name, args);
    call.span = expr.span;
    if let ExprKind::FunctionCall { args, .. } = &mut call.kind {
        for arg in args.iter_mut() {
            if arg.span == crate::span::Span::dummy() {
                arg.span = expr.span;
            }
        }
    }
    call
}

/// `sort($a, $flags)` and its three siblings.
fn lower_flagged_sort(
    ctx: &mut LoweringContext<'_, '_>,
    builtin: &str,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let values = positional_values(args, &["array", "flags"])?;
    let [array, flags] = values.as_slice() else {
        return None;
    };
    if matches!(flags.kind, ExprKind::IntLiteral(0)) {
        let plain = [(*array).clone()];
        return Some(lower_function_call(ctx, &crate::names::Name::unqualified(name), &plain, expr));
    }
    let reverse = matches!(builtin, "rsort" | "arsort");
    let keeps_keys = matches!(builtin, "asort" | "arsort");
    let comparator = synthetic_call(
        prelude::SORT_COMPARATOR_NAME,
        vec![(*flags).clone(), e_bool(reverse)],
        expr,
    );
    let sort = synthetic_call(
        if keeps_keys { "uasort" } else { "usort" },
        vec![(*array).clone(), comparator],
        expr,
    );
    // php's sorts answer `true`; `usort()` lowers to nothing a caller can read.
    let answer = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(sort),
            op: crate::parser::ast::BinOp::StrictNotEq,
            right: Box::new(e_bool(false)),
        },
        expr.span,
    );
    Some(lower_expr(ctx, &answer))
}

/// `array_unique($a, $flags)`.
fn lower_array_unique(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let values = positional_values(args, &["array", "flags"])?;
    let [array, flags] = values.as_slice() else {
        return None;
    };
    if prelude::flags_are_sort_string(flags) {
        let plain = [(*array).clone()];
        return Some(lower_function_call(ctx, &crate::names::Name::unqualified(name), &plain, expr));
    }
    let helper_args = [(*array).clone(), (*flags).clone()];
    Some(call_helper(ctx, prelude::ARRAY_UNIQUE_NAME, &helper_args, expr))
}

/// `array_merge_recursive()` the native hash merge cannot take: other than two arrays, or an
/// operand without a concrete layout. The checker files `php_array()` for exactly those calls.
fn lower_array_merge_recursive(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
        || !filed_type(ctx, expr).is_some_and(PhpType::is_php_array)
    {
        return None;
    }
    let packed = Expr::new(ExprKind::ArrayLiteral(args.to_vec()), expr.span);
    Some(call_helper(
        ctx,
        prelude::ARRAY_MERGE_RECURSIVE_SPREAD_NAME,
        &[packed],
        expr,
    ))
}

/// `array_reverse($gradual, $preserve_keys)`.
fn lower_array_reverse(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let values = positional_values(args, &["array", "preserve_keys"])?;
    let [array, preserve] = values.as_slice() else {
        return None;
    };
    // The checker files `mixed` for exactly a gradual source whose key flag is not a literal
    // `false`; the other shapes keep the native or one-argument gradual path.
    if filed_type(ctx, expr) != Some(&PhpType::Mixed)
        || !matches!(
            materialized_expr_type_for_merge(ctx, array).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        return None;
    }
    let helper_args = [(*array).clone(), (*preserve).clone()];
    Some(call_helper(ctx, prelude::ARRAY_REVERSE_PRESERVE_NAME, &helper_args, expr))
}

/// A spread of unknown length into a builtin the checker answered with its declared type.
fn lower_spread_call(
    ctx: &mut LoweringContext<'_, '_>,
    builtin: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let helper = prelude::spread_helper(builtin)?;
    if !args.iter().any(is_spread_arg)
        || args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
        || !prelude::has_unknown_length_spread(args)
    {
        return None;
    }
    // `f(...$x)` hands the spread's own array over; `f($a, ...$rest)` builds `[$a, ...$rest]`.
    let packed = match args {
        [only] => match &only.kind {
            ExprKind::Spread(inner) => inner.as_ref().clone(),
            _ => return None,
        },
        _ => {
            let entries = args
                .iter()
                .map(|arg| match &arg.kind {
                    ExprKind::Spread(inner) => ArrayEntry::Spread(inner.as_ref().clone()),
                    _ => ArrayEntry::Value(arg.clone()),
                })
                .collect();
            Expr::new(ExprKind::ArrayLiteralMixed(entries), expr.span)
        }
    };
    Some(call_helper(ctx, helper, &[packed], expr))
}

/// `iterator_to_array($scalar)`: php raises the `TypeError` when the call runs, so the helper does.
fn lower_scalar_iterator_source(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let source = args.first()?;
    if matches!(source.kind, ExprKind::NamedArg { .. }) {
        return None;
    }
    if !matches!(
        materialized_expr_type_for_merge(ctx, source).codegen_repr(),
        PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool | PhpType::False | PhpType::Void
    ) {
        return None;
    }
    Some(call_helper(
        ctx,
        prelude::ITERATOR_SOURCE_ERROR_NAME,
        std::slice::from_ref(source),
        expr,
    ))
}

/// `pcntl_sigprocmask($how, $gradual)` and the wait functions: the native lowering reads an
/// array of known layout, so a value known only at run time is checked and listed first.
fn lower_gradual_signal_set(
    ctx: &mut LoweringContext<'_, '_>,
    builtin: &str,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. })) {
        return None;
    }
    let position = if builtin == "pcntl_sigprocmask" { 1 } else { 0 };
    let signals = args.get(position)?;
    if !matches!(
        materialized_expr_type_for_merge(ctx, signals).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return None;
    }
    let checked = synthetic_call(
        prelude::SIGNAL_SET_NAME,
        vec![
            signals.clone(),
            crate::synthetic_class::e_str(builtin),
            crate::synthetic_class::e_int(position as i64 + 1),
        ],
        expr,
    );
    let mut rewritten = args.to_vec();
    rewritten[position] = checked;
    Some(lower_function_call(ctx, &crate::names::Name::unqualified(name), &rewritten, expr))
}
