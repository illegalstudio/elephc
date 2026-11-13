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

/// Builds PHP's `$GLOBALS` array from the program globals this frame can address.
///
/// Main has every top-level PHP variable in its own frame, so the visible-local projection
/// is exactly the global symbol table at that point. Inside a function, only `global $name`
/// aliases and superglobals spelled in this body have initialised global storage to read;
/// ordinary top-level locals cannot be enumerated from a function frame without changing
/// every top-level local into permanent global storage.
pub(super) fn lower_globals_read(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> LoweredValue {
    // `$GLOBALS` is PHP's superglobal view of every program global, from any scope. The
    // runtime table already knows every top-level binding plus literal and runtime-created
    // names, so materialize from it instead of rebuilding a hash from this frame's local
    // slots (which only saw `global` declarations and superglobals).
    ctx.emit_owned_value(
        Op::LoadGlobalsArray,
        Vec::new(),
        None,
        PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        },
        Op::LoadGlobalsArray.default_effects(),
        Some(expr.span),
    )
}

/// Builds the associative array of locals visible at the current lexical call site.
pub(super) fn lower_visible_defined_vars(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> LoweredValue {
    // `visible_local_names` already drops every compiler-generated frame and parser temporary,
    // which matters far beyond `func_*` users: once the backtrace gate fires, EVERY function in
    // the program receives the hidden argument collector.
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
