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
//! - The message reports the call's line (`LoweringContext::call_site_span`); an `int`
//!   binding's precision deprecation reports the parameter's declaration line, which the parser
//!   records per declaration (`install_param_decl_lines`).
//! - Coercive mode rejects every non-scalar (`null`, arrays, objects other than a `Stringable`
//!   bound to `string`, resources); `strict_types=1` additionally requires the exact scalar,
//!   with PHP's single `int`-to-`float` widening.
//! - User calls only (a `call_description` is set): builtins carry `declared_params: false`
//!   and extern calls clear `call_description`, so neither is ever guarded.
//! - `?T`, unions, `mixed` and a `T $p = null` default stay accepting `null`.
//! - The checker records each such argument (`CheckResult::scalar_guard_arg_sites`) so the AST
//!   optimizer keeps the call `may_throw`; both sides share `types::param_binding`'s predicates.

use super::*;
use std::collections::HashMap;

thread_local! {
    /// Start line of each parameter of the program's functions and methods, keyed by the
    /// case-folded callee name the guard message uses (`f`, `C::m`); see
    /// [`install_param_decl_lines`].
    static PARAM_DECL_LINES: std::cell::RefCell<HashMap<String, Vec<u32>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Restores the previously installed parameter lines when dropped.
pub(crate) struct ParamDeclLinesScope {
    previous: Option<HashMap<String, Vec<u32>>>,
}

impl Drop for ParamDeclLinesScope {
    /// Reinstalls the table that was active before this lowering run.
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            PARAM_DECL_LINES.with(|slot| slot.replace(previous));
        }
    }
}

/// Installs, for one lowering run, the line where each declared parameter starts.
///
/// PHP raises the `int` precision deprecation ("Implicit conversion from float ... to int loses
/// precision") while binding the parameter, so it reports the PARAMETER's line, not the call's.
/// The AST keeps no span per parameter; the parser records the lines keyed by declaration.
pub(crate) fn install_param_decl_lines(program: &[Stmt]) -> ParamDeclLinesScope {
    let mut lines = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, .. } => {
                if let Some(params) = crate::parser::stmt::params::param_lines(stmt.span) {
                    lines.insert(crate::names::php_symbol_key(name), params);
                }
            }
            StmtKind::ClassDecl { name, methods, .. } => {
                for method in methods {
                    if let Some(params) = crate::parser::stmt::params::param_lines(method.span) {
                        lines.insert(
                            crate::names::php_symbol_key(&format!("{name}::{}", method.name)),
                            params,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    ParamDeclLinesScope { previous: Some(PARAM_DECL_LINES.with(|slot| slot.replace(lines))) }
}

/// Records the parameter lines of the closure at `span` under its PHP label, which is the
/// callee name its guards report (`{closure:file:line}`).
pub(super) fn register_closure_param_lines(display_name: &str, span: Span) {
    if let Some(lines) = crate::parser::stmt::params::param_lines(span) {
        PARAM_DECL_LINES.with(|slot| {
            slot.borrow_mut().insert(crate::names::php_symbol_key(display_name), lines);
        });
    }
}

/// Returns the line where parameter `index` of `callee` is declared, if known.
fn param_decl_line(callee: &str, index: usize) -> Option<u32> {
    PARAM_DECL_LINES.with(|slot| {
        slot.borrow().get(&crate::names::php_symbol_key(callee)).and_then(|lines| lines.get(index).copied())
    })
}

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
/// existing conversion runs afterwards. Earlier arguments stay published when it throws; with
/// `release_on_refusal`, an owned operand that is not itself published is released before the
/// throw, together with `also_release`: the call's other unpublished owned operands, which no
/// cleanup root covers either (they become extra operands the refusal path releases).
pub(super) fn guard_scalar_param(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    source: GuardSource,
    span: Span,
    release_on_refusal: bool,
    also_release: &[crate::ir::ValueId],
) -> Option<LoweredValue> {
    let declared = guarded_scalar_param(ctx, sig, index, value.value, source)?;
    let mode = match declared {
        PhpType::Str => 's',
        PhpType::Int => 'i',
        PhpType::Float => 'f',
        _ => 'b',
    };
    let mode = if crate::source::current_strict_types() { mode.to_ascii_uppercase() } else { mode };
    // An owned operand no cleanup root covers is released by the refusal itself, so a
    // `TypeError` does not leak it (the accepting path keeps the ordinary ownership flow).
    let mut releases = Vec::new();
    if release_on_refusal && ctx.value_is_owning_temporary(value) {
        releases.push(value.value);
    }
    releases.extend(also_release.iter().copied());
    let release = if releases.is_empty() { "" } else { "r" };
    // Operand 0 is the checked value; with the `r` flag the rest are released on refusal.
    let mut guard_operands = vec![value.value];
    if !releases.is_empty() {
        guard_operands.extend(releases);
    }
    let argument = sig.params.iter().take(index).filter(|(name, _)| {
        name != crate::func_args::HIDDEN_ARGC_PARAM && name != crate::func_args::HIDDEN_ARGS_PARAM
    }).count() + 1;
    let callable = ctx.call_description.clone().unwrap_or_default();
    let source_path = ctx.source_path().unwrap_or("unknown").to_string();
    // PHP's "called in" names the line where the call starts, not the argument's line.
    let line = ctx.call_site_span.map_or(span.line, |call| call.line);
    let encoded = format!(
        "{mode}{release}\u{1f}{callable}(): Argument #{argument} (${}) must be of type {}, \u{1f} given, called in {source_path} on line {line}",
        sig.params[index].0,
        crate::types::diagnostic_names::php_type_name(&declared),
    );
    let data = ctx.intern_string(&encoded);
    if matches!(declared, PhpType::Int | PhpType::Float) {
        // The instruction's span is the line its diagnostics report: an `int` binding's
        // precision deprecation names the parameter's declaration line, as in PHP.
        let diagnostic_span = match declared {
            PhpType::Int => param_decl_line(&callable, index)
                .map_or(span, |line| Span { line, end_line: line.max(span.end_line), ..span }),
            _ => span,
        };
        let converted = ctx.emit_value(
            Op::ScalarParamGuard,
            guard_operands.clone(),
            Some(Immediate::Data(data)),
            declared,
            Op::ScalarParamGuard.default_effects(),
            Some(diagnostic_span),
        );
        // The scalar replaces the box as the call operand, so an owned temporary box has no
        // reader left; a box rooted for the call (a borrow) retires with the call's owners.
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        return Some(converted);
    }
    ctx.emit_void(
        Op::ScalarParamGuard,
        guard_operands,
        Some(Immediate::Data(data)),
        Op::ScalarParamGuard.default_effects(),
        Some(span),
    );
    None
}
