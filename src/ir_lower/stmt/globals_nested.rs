//! Purpose:
//! Lowers nested writes rooted at `$GLOBALS[...]` through a hidden local copy and publishes
//! the mutated container back to program-global storage.
//!
//! Called from:
//! - `crate::ir_lower::stmt` for a `NestedArrayAssign` rooted at `$GLOBALS`.
//!
//! Key details:
//! - The outer key is evaluated before inner keys and the RHS, matching PHP's order.
//! - The hidden local isolates the write from any same-named function local while the final
//!   container still goes back through `StoreGlobal` / `StoreGlobalDynamic`.

use super::*;
use crate::ir_lower::expr::coerce_to_string_at_span;
use crate::ir_lower::ownership::acquire_if_refcounted;

/// Lowers `$GLOBALS[$outer][...] = $value` and returns whether the target matched.
pub(super) fn lower_nested_globals_assign(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    span: Span,
) -> bool {
    let hidden = ctx.declare_synthetic_php_local(PhpType::Mixed);
    let Some((outer_index, rewritten)) = rewrite_globals_prefix(target, &hidden) else {
        return false;
    };
    if !matches!(rewritten.kind, ExprKind::ArrayAccess { .. }) {
        return false;
    }

    let key = lower_expr(ctx, &outer_index);
    let key = coerce_to_string_at_span(ctx, key, Some(outer_index.span));
    let literal_outer = match &outer_index.kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        _ => None,
    };
    let global_value = match &literal_outer {
        Some(name) => {
            let data = ctx.intern_global_name(name);
            ctx.emit_value(
                Op::LoadGlobal,
                Vec::new(),
                Some(Immediate::GlobalName(data)),
                PhpType::Mixed,
                Op::LoadGlobal.default_effects(),
                Some(span),
            )
        }
        None => ctx.emit_value(
            Op::LoadGlobalDynamic,
            vec![key.value],
            Some(Immediate::Bool(false)),
            PhpType::Mixed,
            Op::LoadGlobalDynamic.default_effects(),
            Some(span),
        ),
    };
    ctx.store_local(&hidden, global_value, PhpType::Mixed, Some(span));

    // Keep the runtime key alive across inner keys, the RHS, and any user callbacks.
    let key_local = ctx.declare_synthetic_php_local(PhpType::Str);
    ctx.store_local(&key_local, key, PhpType::Str, Some(span));

    super::nested_array_writes::lower_nested_array_assign(ctx, &rewritten, value, span);

    let updated = ctx.load_local(&hidden, Some(span));
    let updated = acquire_if_refcounted(ctx, updated, Some(span));
    match literal_outer {
        Some(name) => {
            let data = ctx.intern_global_name(&name);
            ctx.emit_void(
                Op::StoreGlobal,
                vec![updated.value],
                Some(Immediate::GlobalName(data)),
                Op::StoreGlobal.default_effects(),
                Some(span),
            );
        }
        None => {
            let key = ctx.load_local(&key_local, Some(span));
            ctx.emit_void(
                Op::StoreGlobalDynamic,
                vec![key.value, updated.value],
                None,
                Op::StoreGlobalDynamic.default_effects(),
                Some(span),
            );
        }
    }
    true
}

/// Replaces the first `$GLOBALS[index]` prefix in `target` with `Variable(hidden)`.
///
/// Returns the outer key expression and the rewritten target. A one-level target rewrites to
/// the bare hidden variable; the caller rejects that shape because it has no nested parent.
fn rewrite_globals_prefix(target: &Expr, hidden: &str) -> Option<(Expr, Expr)> {
    let ExprKind::ArrayAccess { array, index } = &target.kind else {
        return None;
    };
    if matches!(&array.kind, ExprKind::Variable(name) if name == "GLOBALS") {
        let replacement = Expr::new(ExprKind::Variable(hidden.to_string()), target.span);
        return Some(((**index).clone(), replacement));
    }
    let (outer, rewritten_array) = rewrite_globals_prefix(array, hidden)?;
    let rebuilt = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(rewritten_array),
            index: index.clone(),
        },
        target.span,
    );
    Some((outer, rebuilt))
}
