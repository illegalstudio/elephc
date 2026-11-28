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
//!   binding's precision deprecation reports the parameter's declaration file and line, which
//!   the parser records per declaration (`install_param_decl_lines`).
//! - Coercive mode rejects every non-scalar (`null`, arrays, objects other than a `Stringable`
//!   bound to `string`, resources); `strict_types=1` additionally requires the exact scalar,
//!   with PHP's single `int`-to-`float` widening.
//! - User calls only (a `call_description` is set): builtins carry `declared_params: false`
//!   and extern calls clear `call_description`, so neither is ever guarded. Methods of the
//!   internal `ext/uri` prelude classes are skipped in coercive mode (PHP coerces `null` for
//!   them) and checked without a "called in" suffix under `strict_types=1`.
//! - A method whose overrides (subclasses, implementors) declare a different parameter is not
//!   checked: the guard only knows the receiver's static class, PHP binds the selected method.
//! - `?T`, unions, `mixed` and a `T $p = null` default stay accepting `null`.
//! - The checker records each such argument (`CheckResult::scalar_guard_arg_sites`) so the AST
//!   optimizer keeps the call `may_throw`; both sides share `types::param_binding`'s predicates.

use super::*;
use std::collections::HashMap;

thread_local! {
    /// Where each parameter of the program's functions and methods starts (its line, in the
    /// declaring file), keyed by the case-folded callee name the guard message uses (`f`,
    /// `C::m`); see [`install_param_decl_lines`].
    static PARAM_DECL_LINES: std::cell::RefCell<HashMap<String, Vec<Span>>> =
        std::cell::RefCell::new(HashMap::new());
    /// The PHP name of each include-variant function symbol, keyed by the case-folded symbol
    /// (`__elephc_include_variant_<hash>_f` -> `f`); see [`install_param_decl_lines`].
    static VARIANT_PUBLIC_NAMES: std::cell::RefCell<HashMap<String, String>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Restores the previously installed parameter lines and variant names when dropped.
pub(crate) struct ParamDeclLinesScope {
    previous: Option<(HashMap<String, Vec<Span>>, HashMap<String, String>)>,
}

impl Drop for ParamDeclLinesScope {
    /// Reinstalls the tables that were active before this lowering run.
    fn drop(&mut self) {
        if let Some((lines, names)) = self.previous.take() {
            PARAM_DECL_LINES.with(|slot| slot.replace(lines));
            VARIANT_PUBLIC_NAMES.with(|slot| slot.replace(names));
        }
    }
}

/// Returns the PHP function name behind an include-variant symbol, as recorded by the
/// program's `FunctionVariantGroup`s (never parsed back out of the symbol's spelling).
pub(crate) fn variant_public_name(symbol: &str) -> Option<String> {
    VARIANT_PUBLIC_NAMES.with(|slot| slot.borrow().get(&crate::names::php_symbol_key(symbol)).cloned())
}

/// Installs, for one lowering run, where each declared parameter starts.
///
/// PHP raises the `int` precision deprecation ("Implicit conversion from float ... to int loses
/// precision") while binding the parameter, so it reports the PARAMETER's file and line, not
/// the call's. The AST keeps no span per parameter; the parser records the lines keyed by
/// declaration, and the declaration's span names its file (an included one too).
pub(crate) fn install_param_decl_lines(
    program: &[Stmt],
    classes: &HashMap<String, crate::types::ClassInfo>,
) -> ParamDeclLinesScope {
    let mut lines = HashMap::new();
    let mut groups = Vec::new();
    collect_param_decl_lines(program, classes, &mut lines, &mut groups);
    let names = groups.iter().flat_map(|(name, variants)| variants.iter()
        .map(|variant| (crate::names::php_symbol_key(variant), name.clone()))).collect();
    // A function an included file declares is lowered under a generated variant name, while
    // its guards name the PHP function: give that name the variant's sites when every
    // variant declares its parameters on the same lines of the same file.
    for (name, variants) in groups {
        let mut sites = variants.iter().map(|variant| lines.get(&crate::names::php_symbol_key(variant)));
        if let Some(Some(first)) = sites.next() {
            if sites.all(|other| other == Some(first)) {
                let first = first.clone();
                lines.insert(crate::names::php_symbol_key(&name), first);
            }
        }
    }
    ParamDeclLinesScope {
        previous: Some((
            PARAM_DECL_LINES.with(|slot| slot.replace(lines)),
            VARIANT_PUBLIC_NAMES.with(|slot| slot.replace(names)),
        )),
    }
}

/// Records the parameter sites of the declarations in `stmts`, including those an included
/// file, a namespace block or a conditional body declares.
///
/// A class's methods come from the checker's flattened declarations when it has them, so a
/// method a trait supplies is recorded under the using class (`C::m`), the name PHP reports.
fn collect_param_decl_lines(
    stmts: &[Stmt],
    classes: &HashMap<String, crate::types::ClassInfo>,
    lines: &mut HashMap<String, Vec<Span>>,
    groups: &mut Vec<(String, Vec<String>)>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, .. } => {
                if let Some(params) = param_sites(stmt.span) {
                    lines.insert(crate::names::php_symbol_key(name), params);
                }
            }
            StmtKind::ClassDecl { name, methods, .. } | StmtKind::EnumDecl { name, methods, .. } => {
                let methods = classes.get(name).map_or(methods.as_slice(), |info| info.method_decls.as_slice());
                for method in methods {
                    if let Some(params) = param_sites(method.span) {
                        lines.insert(
                            crate::names::php_symbol_key(&format!("{name}::{}", method.name)),
                            params,
                        );
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => collect_param_decl_lines(body, classes, lines, groups),
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_param_decl_lines(then_body, classes, lines, groups);
                for (_, body) in elseif_clauses {
                    collect_param_decl_lines(body, classes, lines, groups);
                }
                if let Some(body) = else_body {
                    collect_param_decl_lines(body, classes, lines, groups);
                }
            }
            StmtKind::FunctionVariantGroup { name, variants } => {
                groups.push((name.clone(), variants.clone()));
            }
            _ => {}
        }
    }
}

/// The start of each parameter of the declaration at `decl`, as spans in the declaring file.
fn param_sites(decl: Span) -> Option<Vec<Span>> {
    crate::parser::stmt::params::param_lines(decl).map(|lines| {
        lines.into_iter().map(|line| Span::new_in_source(line, decl.col, decl.source_id())).collect()
    })
}

/// Records the parameter lines of the closure at `span` under its PHP label, which is the
/// callee name its guards report (`{closure:file:line}`).
pub(super) fn register_closure_param_lines(display_name: &str, span: Span) {
    if let Some(sites) = param_sites(span) {
        PARAM_DECL_LINES.with(|slot| {
            slot.borrow_mut().insert(crate::names::php_symbol_key(display_name), sites);
        });
    }
}

/// Returns where parameter `index` of `callee` is declared, if known.
fn param_decl_site(callee: &str, index: usize) -> Option<Span> {
    PARAM_DECL_LINES.with(|slot| {
        slot.borrow().get(&crate::names::php_symbol_key(callee)).and_then(|sites| sites.get(index).copied())
    })
}

/// Spells the callee of a parameter `TypeError`: an anonymous class's method is named by the
/// class alone (`class@anonymous`, `Parent@anonymous`), as PHP prints it, because the NUL in
/// the class's runtime name ends the string before `::method`.
fn guard_callee_label(ctx: &LoweringContext<'_, '_>, callable: &str) -> String {
    match callable.split_once("::") {
        Some((class, _)) if crate::parser::is_anonymous_class_name(class) => ctx.php_class_label(class),
        _ => callable.to_string(),
    }
}

/// Names a method the way PHP's parameter `TypeError` does: `Declaring::method`, the class whose
/// body declares the implementation the receiver's class dispatches to (`A::m` for an inherited
/// `B::m`, the using class for a trait method), with the method's declared spelling. That is
/// also the key [`install_param_decl_lines`] records the parameter lines under.
pub(super) fn method_guard_description(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
    is_static: bool,
) -> String {
    let class_name = class_name.trim_start_matches('\\');
    let key = crate::names::php_symbol_key(method);
    let declaring = ctx.classes.get(class_name).and_then(|info| {
        let implementations = if is_static {
            &info.static_method_impl_classes
        } else {
            &info.method_impl_classes
        };
        implementations.get(&key)
    });
    let declaring = declaring.map(String::as_str).unwrap_or(class_name);
    let spelled = ctx.classes.get(declaring)
        .and_then(|info| info.method_decls.iter().find(|decl| crate::names::php_symbol_key(&decl.name) == key))
        .map_or(method, |decl| decl.name.as_str());
    format!("{declaring}::{spelled}")
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
    let description = ctx.call_description.as_deref()?;
    // The `ext/uri` classes are internal in PHP: in coercive mode a `null` argument is coerced
    // with a deprecation rather than refused, so their prelude methods keep the binding they
    // had before this guard existed. `strict_types=1` checks them like any callee.
    if is_internal_uri_callee(description) && !crate::source::current_strict_types() {
        return None;
    }
    let declared = crate::types::param_binding::runtime_scalar_guard_param(sig, index)?;
    if matches!(declared, PhpType::Union(_)) {
        // Scalar unions use the multi-member guard in call argument coercion, not the
        // single-scalar conversion instruction.
        return None;
    }
    // The check uses the signature lowering resolved from the receiver's static class, but PHP
    // binds the method the object selects at run time: an override may accept more.
    if !overrides_keep_scalar_param(ctx, description, index, declared) {
        return None;
    }
    if !matches!(
        ctx.builder.value_type(value),
        IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)
    ) {
        return None;
    }
    let needed = match source {
        GuardSource::Recorded(span) if ctx.pipe_guard_arg_span != Some(span) => {
            ctx.scalar_guard_arg_sites.contains(&span)
        }
        GuardSource::Recorded(_) | GuardSource::Unrecorded => crate::types::param_binding::runtime_scalar_guard_needed(
            declared,
            &ctx.builder.value_php_type(value),
            crate::source::current_strict_types(),
        ),
        GuardSource::Skip => false,
    };
    needed.then(|| declared.clone())
}

/// Returns whether `description` (`Class::method`) names a method of an internal URI class.
fn is_internal_uri_callee(description: &str) -> bool {
    description
        .rsplit_once("::")
        .is_some_and(|(class, _)| crate::uri_prelude::is_internal_uri_class(class))
}

/// Returns whether the method `description` names (`Declaring::method`) is the one PHP binds
/// at run time for every receiver the call can see, so its parameter check is exact.
///
/// The check only knows the receiver's static class. When a subclass, or another class
/// implementing the interface, overrides the method, PHP checks the selected override instead:
/// a wider parameter (`mixed`, `?string`, a union) accepts values the declaring method refuses,
/// and even an identical one names its own class in the `TypeError`. Such a call is left
/// unchecked. A plain function (no `::`) has no overrides.
fn overrides_keep_scalar_param(
    ctx: &LoweringContext<'_, '_>,
    description: &str,
    index: usize,
    declared: &PhpType,
) -> bool {
    let Some((declaring, method)) = description.rsplit_once("::") else {
        return true;
    };
    let declaring_key = crate::names::php_symbol_key(declaring);
    let method_key = crate::names::php_symbol_key(method);
    let is_declaring = |class: &str| crate::names::php_symbol_key(class) == declaring_key;
    ctx.classes.iter().all(|(class_name, info)| {
        let related = super::property_access::class_extends_class(ctx, class_name, declaring)
            || info.interfaces.iter().any(|interface| is_declaring(interface));
        if !related {
            return true;
        }
        let selected = [
            (info.methods.get(&method_key), info.method_impl_classes.get(&method_key)),
            (info.static_methods.get(&method_key), info.static_method_impl_classes.get(&method_key)),
        ];
        selected.into_iter().all(|(sig, implementation)| match sig {
            None => true,
            Some(sig) => implementation.map_or(is_declaring(class_name), |class| is_declaring(class))
                && crate::types::param_binding::runtime_scalar_guard_param(sig, index) == Some(declared),
        })
    })
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
    let argument = sig.params.iter().take(index).filter(|(name, _)| {
        name != crate::func_args::HIDDEN_ARGC_PARAM && name != crate::func_args::HIDDEN_ARGS_PARAM
    }).count() + 1;
    let callable = ctx.call_description.clone().unwrap_or_default();
    let source_path = ctx.source_path().unwrap_or("unknown").to_string();
    // PHP's "called in" names the line where the call starts, not the argument's line. An
    // internal method's refusal (a URI method under `strict_types=1`) has no such suffix.
    let line = ctx.call_site_span.map_or(span.line, |call| call.line);
    let prefix = format!(
        "{}(): Argument #{argument} (${}) must be of type {}, ",
        guard_callee_label(ctx, &callable),
        sig.params[index].0,
        crate::types::diagnostic_names::php_type_name(&declared),
    );
    let suffix = if is_internal_uri_callee(&callable) {
        " given".to_string()
    } else {
        format!(" given, called in {source_path} on line {line}")
    };
    // The instruction's span is the site its diagnostics report: an `int` binding's precision
    // deprecation names the parameter's declaration file and line, as in PHP.
    let diagnostic_span = match declared {
        PhpType::Int => param_decl_site(&callable, index).unwrap_or(span),
        _ => span,
    };
    emit_scalar_guard(
        ctx, value, &declared, &prefix, &suffix, span,
        ScalarGuardEmit { release_on_refusal, also_release: also_release.to_vec(), diagnostic_span },
    )
}

/// Per-site options of [`emit_scalar_guard`].
#[derive(Clone)]
pub(crate) struct ScalarGuardEmit {
    /// Release an owned operand no cleanup root covers when the check refuses it.
    pub(crate) release_on_refusal: bool,
    /// The call's other unpublished owned operands, also released when the check refuses.
    pub(crate) also_release: Vec<crate::ir::ValueId>,
    /// The span the instruction's diagnostics (the int precision deprecation) report.
    pub(crate) diagnostic_span: Span,
}

/// Emits one `Op::ScalarParamGuard` checking `value` against the declared scalar `declared`
/// under the current file's `strict_types` mode; a refused value throws `TypeError` with
/// `prefix` + its PHP type name + `suffix`.
///
/// Shared by argument binding and declared return boundaries. For `int`/`float` it returns the
/// converted scalar and releases an owned temporary box, which has no reader left.
pub(crate) fn emit_scalar_guard(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    declared: &PhpType,
    prefix: &str,
    suffix: &str,
    span: Span,
    options: ScalarGuardEmit,
) -> Option<LoweredValue> {
    let declared = declared.clone();
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
    if options.release_on_refusal && ctx.value_is_owning_temporary(value) {
        releases.push(value.value);
    }
    releases.extend(options.also_release.iter().copied());
    let release = if releases.is_empty() { "" } else { "r" };
    // Operand 0 is the checked value; with the `r` flag the rest are released on refusal.
    let mut guard_operands = vec![value.value];
    guard_operands.extend(releases);
    let encoded = format!("{mode}{release}\u{1f}{prefix}\u{1f}{suffix}");
    let data = ctx.intern_string(&encoded);
    if matches!(declared, PhpType::Int | PhpType::Float) {
        let converted = ctx.emit_value(
            Op::ScalarParamGuard,
            guard_operands.clone(),
            Some(Immediate::Data(data)),
            declared,
            Op::ScalarParamGuard.default_effects(),
            Some(options.diagnostic_span),
        );
        // The scalar replaces the box, so an owned temporary box has no reader left; a box
        // rooted for a call (a borrow) retires with that call's owners.
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
