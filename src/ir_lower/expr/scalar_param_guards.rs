//! Purpose:
//! Rejects a runtime-typed argument that a declared non-nullable scalar parameter refuses,
//! raising PHP's `f(): Argument #N ($name) must be of type T, U given, called in ...`.
//!
//! Called from:
//! - `call_arg_coercion::coerce_operands_to_params_guarded`, after every argument of the call
//!   was evaluated (positional, named and spread arguments alike), so a later argument's side
//!   effects precede the `TypeError` as in PHP.
//! - `call_arg_coercion::coerce_scalar_arg_to_param_storage`, at argument time, only for
//!   entries that do not normalize operands afterwards.
//!
//! Key details:
//! - Only boxed `mixed`/union storage is checked; a statically typed argument emits nothing.
//! - The check is one `Op::ScalarParamGuard` (no new basic block): codegen keeps the tag test
//!   inline and the `TypeError` construction in the function's out-of-line cold section. For
//!   `int`/`float` it also yields the converted value (PHP's numeric-string and float rules).
//! - The message reports the call's line (`LoweringContext::call_site_span`).
//! - Coercive mode rejects every non-scalar (`null`, arrays, objects other than a `Stringable`
//!   bound to `string`, resources); `strict_types=1` additionally requires the exact scalar,
//!   with PHP's single `int`-to-`float` widening.
//! - User calls only (a `call_description` is set): builtins carry `declared_params: false`
//!   and extern calls clear `call_description`, so neither is ever guarded.
//! - `?T`, unions, `mixed` and a `T $p = null` default stay accepting `null`.
//! - The checker records each such argument (`CheckResult::scalar_guard_arg_sites`) so the AST
//!   optimizer keeps the call `may_throw`; both sides share `types::param_binding`'s predicates.

use super::*;

/// Where the decision to guard one operand comes from.
#[derive(Clone, Copy, Debug)]
pub(super) enum GuardSource {
    /// The argument expression at this span: guarded only when the checker recorded it in
    /// `scalar_guard_arg_sites`, so a value the checker proved scalar costs nothing even when
    /// its storage widened to a boxed cell (a loop counter promoted for overflow, for example).
    Recorded(Span),
    /// An operand with no checker record of its own (a named argument, or an element of a
    /// spread): decided from the value's PHP type.
    Unrecorded,
    /// Never guarded (an omitted parameter's default, or a variadic tail).
    Skip,
}

/// Returns the declared scalar a runtime-typed argument must be checked against.
///
/// `None` means no guard is needed: the call is not a user call, the parameter accepts `null`
/// or is not a plain scalar (`param_binding::runtime_scalar_guard_param`), the value is not
/// boxed, or `source` says the value cannot hold a refused type.
pub(super) fn guarded_scalar_param(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: crate::ir::ValueId,
    source: GuardSource,
) -> Option<PhpType> {
    ctx.call_description.as_ref()?;
    let declared = crate::types::param_binding::runtime_scalar_guard_param(sig, index)?;
    if !matches!(
        ctx.builder.value_type(value),
        IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)
    ) {
        return None;
    }
    let needed = match source {
        GuardSource::Recorded(span) => ctx.scalar_guard_arg_sites.contains(&span),
        GuardSource::Unrecorded => crate::types::param_binding::runtime_scalar_guard_needed(
            declared,
            &ctx.builder.value_php_type(value),
            crate::source::current_strict_types(),
        ),
        GuardSource::Skip => false,
    };
    needed.then(|| declared.clone())
}

/// Returns whether positional argument lowering at the current depth leaves guarded operands
/// boxed for `coerce_operands_to_params_guarded`, and with which source policy.
///
/// PHP evaluates every argument before the callee checks any parameter type, so a later
/// argument's side effects happen before the `TypeError`. Deferring the check (and the
/// `mixed`-to-`string` conversion it protects) to parameter-order normalization gives that
/// order; the flag is keyed by expression depth so nested calls inside an argument keep
/// their own policy.
pub(super) fn deferred_guard_source(ctx: &LoweringContext<'_, '_>, arg: &Expr) -> Option<GuardSource> {
    match ctx.scalar_guard_deferral {
        Some((depth, unrecorded)) if depth == ctx.expression_depth => Some(if unrecorded {
            GuardSource::Unrecorded
        } else {
            GuardSource::Recorded(arg.span)
        }),
        _ => None,
    }
}

/// Emits the runtime check for one argument as a single `Op::ScalarParamGuard`.
///
/// The instruction adds no basic block: codegen keeps the tag test inline and the rejection in
/// the function's cold section, which throws PHP's `TypeError` with the value's type name.
/// For an `int` or `float` parameter the instruction also yields the converted scalar, with
/// PHP's numeric-string rules, the lossy-conversion deprecation and the range check, and the
/// caller passes it instead of the box. For `string` and `bool` it yields nothing and the
/// existing conversion runs afterwards. Earlier arguments stay published when it throws.
pub(super) fn guard_scalar_param(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    source: GuardSource,
    span: Span,
) -> Option<LoweredValue> {
    let declared = guarded_scalar_param(ctx, sig, index, value.value, source)?;
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
    let source_path = ctx.source_path().unwrap_or("unknown").to_string();
    // PHP's "called in" names the line where the call starts, not the argument's line.
    let line = ctx.call_site_span.map_or(span.line, |call| call.line);
    let encoded = format!(
        "{mode}\u{1f}{callable}(): Argument #{argument} (${}) must be of type {}, \u{1f} given, called in {source_path} on line {line}",
        sig.params[index].0,
        crate::types::diagnostic_names::php_type_name(&declared),
    );
    let data = ctx.intern_string(&encoded);
    if matches!(declared, PhpType::Int | PhpType::Float) {
        return Some(ctx.emit_value(
            Op::ScalarParamGuard,
            vec![value.value],
            Some(Immediate::Data(data)),
            declared,
            Op::ScalarParamGuard.default_effects(),
            Some(span),
        ));
    }
    ctx.emit_void(
        Op::ScalarParamGuard,
        vec![value.value],
        Some(Immediate::Data(data)),
        Op::ScalarParamGuard.default_effects(),
        Some(span),
    );
    None
}
