//! Purpose:
//! Lowers literal-dispatched `filter_var()` calls to a synthetic per-filter
//! builtin name that `crate::codegen::lower_inst::builtins::filter` matches
//! on directly, so the EIR backend never has to re-derive the filter id/flags
//! from operand structure (which would depend on whether `--ir-opt` folded a
//! `FILTER_NULL_ON_FAILURE | FILTER_REQUIRE_SCALAR`-style flags expression).
//!
//! Called from:
//! - `crate::ir_lower::expr::lower_function_call()`.
//!
//! Key details:
//! - The checker (`crate::types::checker::builtins::system::check_builtin`)
//!   already rejects non-literal filter ids/flags and unsupported filters/flags
//!   before lowering ever runs, so this module can assume every call it sees
//!   resolves statically. It still returns `None` defensively (falling through
//!   to the generic builtin-call path, which surfaces a loud "unsupported
//!   builtin" EIR error) if that assumption is somehow violated.
//! - Only plain positional 1-3 argument calls are handled; a named/spread call
//!   falls through to the same defensive `None` path.

use crate::names::php_symbol_key;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::filter_constants::FILTER_INT_CONSTANTS;
use crate::types::PhpType;

use super::super::context::{LoweredValue, LoweringContext};

/// Lowers a literal-dispatched `filter_var()` call to its synthetic per-filter
/// builtin, or returns `None` to defer to the generic builtin-call path.
pub(super) fn lower_static_filter_var(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "filter_var" {
        return None;
    }
    if !(1..=3).contains(&args.len()) {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(super::is_spread_arg) {
        return None;
    }
    // A dynamic (runtime, non-constant) `$filter` cannot select a synthetic per-filter builtin at
    // compile time. Route it to a `__elephc_filter_var_dyn*` prelude helper (injected whenever
    // `filter_var` is referenced — see `crate::filter_var_prelude`), which dispatches the runtime
    // id to the SAME literal per-filter lowering. The checker accepts this shape as `Mixed`.
    //
    // A statically array-typed `$options` (`['flags' => ...]` literal, or an array-typed variable)
    // is routed to the `array`-parameter shim, so the array never crosses a `mixed` parameter (an
    // array boxed into `mixed` is a pre-existing elephc gap — element access fatals at runtime).
    // Everything else (int flags, no options, or a `mixed`-typed options) takes the int-flags
    // helper; a `mixed`-typed options that holds an array at runtime is the documented residual.
    if args.len() >= 2 && static_int_value(&args[1]).is_none() {
        let helper = if args.len() == 3 && options_arg_is_array_typed(ctx, &args[2]) {
            crate::filter_var_prelude::DYN_FILTER_VAR_ARR_NAME
        } else {
            crate::filter_var_prelude::DYN_FILTER_VAR_NAME
        };
        return Some(super::lower_function_call(
            ctx,
            &crate::names::Name::unqualified(helper),
            args,
            expr,
        ));
    }
    let filter_id = if args.len() >= 2 {
        static_int_value(&args[1])?
    } else {
        516 // FILTER_DEFAULT (== FILTER_UNSAFE_RAW)
    };
    // URL, EMAIL, DOMAIN and CALLBACK run in `__elephc_filter_var_ext(value, filter, options)`.
    if crate::filter_ext_prelude::EXTENDED_FILTER_IDS.contains(&filter_id) {
        return Some(super::lower_function_call(
            ctx,
            &crate::names::Name::unqualified(crate::filter_ext_prelude::FILTER_VAR_EXT_NAME),
            args,
            expr,
        ));
    }
    // FILTER_VALIDATE_INT with a compile-time integer range constraint
    // (`['options' => ['min_range' => C, 'max_range' => C]]`). The checker accepts the
    // SAME shape via `static_filter_int_range_options`. Lowers to `filter_var$int_range`
    // carrying the value plus the resolved bounds (absent bound → PHP_INT_MIN/MAX), so
    // the codegen INT filter can reject out-of-range results.
    if filter_id == 257 && args.len() == 3 {
        if let Some(range) =
            crate::types::filter_constants::static_filter_int_range_options(&args[2])
        {
            let null_on_failure = range.flags & 134_217_728 != 0;
            let synthetic_name = if null_on_failure {
                "filter_var$int_range_nof"
            } else {
                "filter_var$int_range"
            };
            let value = super::lower_expr(ctx, &args[0]);
            let min = super::lower_int_literal(ctx, range.min_range.unwrap_or(i64::MIN), expr);
            let max = super::lower_int_literal(ctx, range.max_range.unwrap_or(i64::MAX), expr);
            return Some(super::emit_builtin_call_value(
                ctx,
                synthetic_name,
                vec![value.value, min.value, max.value],
                PhpType::Mixed,
                expr.span,
                None,
            ));
        }
    }
    // Resolve flags from a constant int OR a `['flags' => <const>]`-only array literal, using the
    // SAME shared resolver the checker uses so an accepted call always lowers (the checker already
    // rejected every array carrying an `options` entry or a non-constant flags value).
    let flags = if args.len() == 3 {
        crate::types::filter_constants::static_filter_options_flags(&args[2])?
    } else {
        0
    };
    let null_on_failure = flags & 134_217_728 != 0;
    let synthetic_name = match (filter_id, null_on_failure) {
        // DEFAULT/UNSAFE_RAW only "fails" on array input (no REQUIRE_ARRAY/FORCE_ARRAY);
        // every scalar/null input always passes (php-verified), so the null_on_failure
        // variant only changes the array-input failure result (false vs null).
        (516, false) => "filter_var$default",
        (516, true) => "filter_var$default_nof",
        (257, false) => "filter_var$int",
        (257, true) => "filter_var$int_nof",
        (259, false) => "filter_var$float",
        (259, true) => "filter_var$float_nof",
        (258, false) => "filter_var$bool",
        (258, true) => "filter_var$bool_nof",
        // VALIDATE_IP: FILTER_FLAG_IPV4 (1048576) / FILTER_FLAG_IPV6 (2097152)
        // restrict the accepted family; both set or neither set accept either
        // family (php-verified — see `crate::types::filter_constants`'s module
        // doc for the exact matrix).
        (275, _) => {
            let ipv4 = flags & 1_048_576 != 0;
            let ipv6 = flags & 2_097_152 != 0;
            let family = match (ipv4, ipv6) {
                (true, false) => "ip4",
                (false, true) => "ip6",
                _ => "ip",
            };
            let value = super::lower_expr(ctx, &args[0]);
            let synthetic_name = if null_on_failure {
                format!("filter_var${family}_nof")
            } else {
                format!("filter_var${family}")
            };
            return Some(super::emit_builtin_call_value(
                ctx,
                &synthetic_name,
                vec![value.value],
                PhpType::Mixed,
                expr.span,
                None,
            ));
        }
        _ => return None,
    };
    let value = super::lower_expr(ctx, &args[0]);
    Some(super::emit_builtin_call_value(
        ctx,
        synthetic_name,
        vec![value.value],
        PhpType::Mixed,
        expr.span,
        None,
    ))
}

/// Returns whether the `filter_var()` `$options` argument is statically array-typed, so the
/// dynamic-`$filter` routing can send it to the `array`-parameter helper instead of the `mixed`
/// one. Recognizes an array literal directly, or a plain variable whose lowered logical type is an
/// array/associative array. A `mixed`-typed variable returns `false` (routed to the int helper),
/// which is the documented residual for a `mixed`-boxed array.
fn options_arg_is_array_typed(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => true,
        ExprKind::Variable(name) => matches!(
            ctx.local_types.get(name),
            Some(PhpType::Array(_) | PhpType::AssocArray { .. })
        ),
        _ => false,
    }
}

/// Attempts to evaluate an expression as a static integer at compile time
/// against the `ext/filter` constant table. Mirrors
/// `crate::types::checker::builtins::system`'s `filter_static_int_value`
/// (kept as a small, independently-testable duplicate since the checker and
/// ir_lower layers walk the same `&Expr` AST but do not share a helper module).
fn static_int_value(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => FILTER_INT_CONSTANTS
            .iter()
            .find_map(|(constant, value)| (*constant == name.as_str()).then_some(*value)),
        ExprKind::Negate(inner) => static_int_value(inner).map(|value| -value),
        ExprKind::BinaryOp { left, op, right } => {
            let left = static_int_value(left)?;
            let right = static_int_value(right)?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => None,
    }
}
