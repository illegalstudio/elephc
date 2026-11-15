//! Purpose:
//! Rejects a runtime-typed argument that a declared non-nullable scalar parameter refuses,
//! raising PHP's `f(): Argument #N ($name) must be of type T, U given, called in ...`.
//!
//! Called from:
//! - `call_arg_coercion::coerce_scalar_arg_to_param_storage` (positional arguments).
//! - `call_arg_coercion::coerce_operands_to_params_guarded` (named and associative-spread
//!   arguments, which are reordered after source evaluation).
//!
//! Key details:
//! - Only boxed `mixed`/union storage is checked; a statically typed argument emits nothing.
//! - The check is one `Op::ScalarParamGuard` (no new basic block): codegen keeps the tag test
//!   inline and the `TypeError` construction in the function's out-of-line cold section.
//! - Coercive mode rejects every non-scalar (`null`, arrays, objects other than a `Stringable`
//!   bound to `string`, resources); `strict_types=1` additionally requires the exact scalar,
//!   with PHP's single `int`-to-`float` widening.
//! - User calls only (a `call_description` is set): builtins carry `declared_params: false`
//!   and extern calls clear `call_description`, so neither is ever guarded.
//! - `?T`, unions, `mixed` and a `T $p = null` default stay accepting `null`.
//! - The checker records each such argument (`CheckResult::scalar_guard_arg_sites`) so the AST
//!   optimizer keeps the call `may_throw`; both sides share `types::param_binding`'s predicates.

use super::*;

/// Returns the declared scalar a runtime-typed argument must be checked against.
///
/// `None` means no guard is needed: the call is not a user call, the parameter accepts `null`
/// or is not a plain scalar (`param_binding::runtime_scalar_guard_param`), or the value is not
/// boxed. With the source expression in hand the checker decides: it recorded the argument in
/// `scalar_guard_arg_sites` only when its static type can hold a rejected value, which also
/// covers locals whose storage widened to a boxed cell while their checked type stayed scalar
/// (such as a loop counter promoted for overflow). Reordered operands without an expression
/// fall back to the value's own PHP type.
pub(super) fn guarded_scalar_param(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: crate::ir::ValueId,
    arg: Option<&Expr>,
) -> Option<PhpType> {
    ctx.call_description.as_ref()?;
    let declared = crate::types::param_binding::runtime_scalar_guard_param(sig, index)?;
    if !matches!(
        ctx.builder.value_type(value),
        IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)
    ) {
        return None;
    }
    let needed = match arg {
        Some(arg) => ctx.scalar_guard_arg_sites.contains(&arg.span),
        None => crate::types::param_binding::runtime_scalar_guard_needed(
            declared,
            &ctx.builder.value_php_type(value),
            crate::source::current_strict_types(),
        ),
    };
    needed.then(|| declared.clone())
}

/// Emits the runtime check for one argument as a single `Op::ScalarParamGuard`.
///
/// The instruction leaves `value` untouched for the existing scalar coercion and adds no
/// basic block: codegen keeps the tag test inline and the rejection in the function's cold
/// section, which throws PHP's `TypeError` with the value's type name. Earlier arguments stay
/// published for cleanup when it throws.
pub(super) fn guard_scalar_param(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    arg: Option<&Expr>,
    span: Span,
) {
    let Some(declared) = guarded_scalar_param(ctx, sig, index, value.value, arg) else {
        return;
    };
    let mode = match declared {
        PhpType::Str => 's',
        PhpType::Int => 'i',
        PhpType::Float => 'f',
        _ => 'b',
    };
    let mode = if crate::source::current_strict_types() { mode.to_ascii_uppercase() } else { mode };
    let argument = sig.params.iter().take(index).filter(|(name, _)| {
        name != crate::func_args::HIDDEN_ARGC_PARAM && name != crate::func_args::HIDDEN_ARGS_PARAM
    }).count() + 1;
    let callable = ctx.call_description.clone().unwrap_or_default();
    let source = ctx.source_path().unwrap_or("unknown").to_string();
    let encoded = format!(
        "{mode}\u{1f}{callable}(): Argument #{argument} (${}) must be of type {}, \u{1f} given, called in {source} on line {}",
        sig.params[index].0,
        crate::types::diagnostic_names::php_type_name(&declared),
        span.line,
    );
    let data = ctx.intern_string(&encoded);
    ctx.emit_void(
        Op::ScalarParamGuard,
        vec![value.value],
        Some(Immediate::Data(data)),
        Op::ScalarParamGuard.default_effects(),
        Some(span),
    );
}
