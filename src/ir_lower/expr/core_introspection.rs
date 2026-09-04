//! Purpose:
//! Lowers PHP Core introspection operations that require the current lexical scope.
//!
//! Called from:
//! - `super::function_calls::lower_function_call()` before registry lowering.
//!
//! Key details:
//! - `get_defined_vars()` supports direct calls and PHP's explicit `call_user_func*` forms.

use super::*;

/// Builds `get_defined_vars()` from the definitely visible locals at its direct call site.
pub(super) fn lower_get_defined_vars(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "get_defined_vars" || !args.is_empty() {
        return None;
    }
    Some(lower_visible_defined_vars(ctx, expr))
}

/// Builds the associative array of locals visible at the current lexical call site.
pub(super) fn lower_visible_defined_vars(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> LoweredValue {
    let names = ctx.visible_local_names();
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(names.len() as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    for name in names {
        let key = lower_string_literal(ctx, &name, expr);
        let value = ctx.load_local(&name, Some(expr.span));
        let value = box_value_as_mixed(ctx, value, expr.span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(expr.span),
        );
    }
    hash
}
