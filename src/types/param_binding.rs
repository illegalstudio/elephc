//! Purpose:
//! Owns the PHP parameter-binding rules that let a declared parameter accept an argument
//! of a different type: coercive scalar binding (`string $s` accepting `42`), weak
//! `Stringable` object conversion, callable-name strings (`callable $f` accepting
//! `"strtoupper"`), and the `declare(strict_types=1)` mode that switches those conversions off.
//!
//! Called from:
//! - `crate::types::checker::functions::resolution` (acceptance + diagnostics)
//! - `crate::ir_lower::expr` (the matching argument rewrite)
//!
//! Key details:
//! - `strict_types` is decided by the file the *call site* is written in, not the file the
//!   callee is declared in, so the rejection below is driven by the checker's current statement
//!   (`crate::parser::ast::Stmt::strict_types`) and never by the callee's signature.
//! - Under `strict_types=1` the checker rejects the call outright, so EIR lowering never reaches
//!   the matching rewrite for a strict-mode violation. The rewrites stay coercive-only.
//! - The checker and EIR lowering MUST agree: every binding the checker accepts here is
//!   rewritten by lowering through `rewrite_param_bound_arg`, and nothing else is. A binding
//!   accepted without a matching rewrite would pass raw storage into a differently typed
//!   parameter slot.
//! - Coercion is expressed as the equivalent explicit PHP cast (`(string)`, `(bool)`) or as a
//!   replacement literal, so the runtime result is whatever elephc already produces for that
//!   cast — no separate conversion semantics to keep in sync.
//! - For runtime values, the generic classifier below permits only total coercions (every
//!   source value converts without a diagnostic). Partial coercions (`int $i` from a float or string) are limited to
//!   compile-time constants, because PHP signals their failure modes with a runtime
//!   `Deprecated:` notice or a `TypeError` and elephc has neither channel at a call boundary.
//! - Selected enum unpacks add runtime guards and precision diagnostics through EIR, then use the sibling scalar plan
//!   before parameter storage erases the declared member types.

use crate::parser::ast::{CallableTarget, CastType, Expr, ExprKind, StaticReceiver};
use crate::names::Name;
use crate::numeric_string::{scan_numeric_prefix, NumericScan};
use crate::types::PhpType;

mod union;
pub(crate) use union::{
    runtime_int_binding_needs_diagnostics, runtime_scalar_param_binding,
    runtime_strict_param_acceptance, runtime_weak_int_param_acceptance,
};

/// Extracts the one named object class a value of type `actual` could still be at this boundary.
///
/// A union naming SEVERAL object classes has no single class for the runtime guard to test, so
/// [`nominal_object_boundary_target`] turns it away. The value crossing the boundary narrows it,
/// though: a class unrelated to the argument's own static class can never be what arrives, and
/// PHP's single inheritance makes "unrelated" decidable. Symfony's
/// `ViewEvent::__construct(ControllerArgumentsMetadata|ControllerArgumentsEvent|null)` is called
/// with a `?ControllerMetadata`, and only `ControllerArgumentsMetadata` descends from it — so the
/// existing one-class guard is exactly the check PHP performs there.
///
/// `related` answers whether two class names are the same or one descends from the other. Returns
/// `None` unless exactly one member survives, leaving every genuinely ambiguous union refused.
pub(crate) fn nominal_object_boundary_target_for_source(
    expected: &PhpType,
    actual: &PhpType,
    related: &dyn Fn(&str, &str) -> bool,
) -> Option<String> {
    let actual_class = single_named_object_class(actual)?;
    let PhpType::Union(members) = expected else {
        return None;
    };
    let mut surviving = None;
    for member in members {
        match member {
            PhpType::Void | PhpType::Never => {}
            PhpType::Object(name) if !name.trim_start_matches('\\').is_empty() => {
                if !related(name.trim_start_matches('\\'), &actual_class) {
                    continue;
                }
                if surviving.is_some() {
                    return None;
                }
                surviving = Some(name.clone());
            }
            _ => return None,
        }
    }
    surviving
}

/// Returns the one named object class a type describes, seeing through a nullable union.
fn single_named_object_class(ty: &PhpType) -> Option<String> {
    match ty {
        PhpType::Object(name) if !name.trim_start_matches('\\').is_empty() => {
            Some(name.trim_start_matches('\\').to_string())
        }
        PhpType::Union(members) => {
            let mut found = None;
            for member in members {
                match member {
                    PhpType::Void | PhpType::Never => {}
                    PhpType::Object(name) if !name.trim_start_matches('\\').is_empty() => {
                        if found.is_some() {
                            return None;
                        }
                        found = Some(name.trim_start_matches('\\').to_string());
                    }
                    _ => return None,
                }
            }
            found
        }
        _ => None,
    }
}

/// Extracts the sole named object accepted by a nominal object boundary.
///
/// Nullable object unions are supported because PHP preserves null while checking every object
/// value against the named class or interface. Any union with another value category is left to
/// the ordinary compatibility and gradual-boundary rules.
pub(crate) fn nominal_object_boundary_target(expected: &PhpType) -> Option<String> {
    match expected {
        PhpType::Object(name) if !name.trim_start_matches('\\').is_empty() => Some(name.clone()),
        PhpType::Union(members) => {
            let mut target = None;
            for member in members {
                match member {
                    PhpType::Void | PhpType::Never => {}
                    PhpType::Object(name) if !name.trim_start_matches('\\').is_empty() => {
                        if target.as_ref().is_some_and(|existing| existing != name) {
                            return None;
                        }
                        target = Some(name.clone());
                    }
                    _ => return None,
                }
            }
            target
        }
        _ => None,
    }
}

/// Returns whether a known object can cross `expected` only after a runtime nominal check.
pub(crate) fn object_requires_runtime_nominal_guard(
    expected: &PhpType,
    actual: &PhpType,
) -> bool {
    matches!(actual.codegen_repr(), PhpType::Object(_))
        && nominal_object_boundary_target(expected).is_some()
}

/// Returns whether a gradual value needs a runtime object-or-null return-boundary check.
///
/// This is deliberately separate from parameter binding: a union such as `Object|false` is
/// represented as boxed Mixed at runtime, so a declared nullable-object return must validate its
/// active tag and class at either a parameter or return boundary.
pub(crate) fn gradual_object_requires_runtime_nominal_guard(
    expected: &PhpType,
    actual: &PhpType,
) -> bool {
    matches!(actual.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && nominal_object_boundary_target(expected).is_some()
}

/// Returns whether a nullable integer must be checked before crossing an `int` parameter.
///
/// PHP accepts the program and throws `TypeError` only if the runtime value is null. Elephc's
/// compact `TaggedScalar` representation carries exactly the payload and null tag needed to make
/// that decision at the call boundary.
pub(crate) fn nullable_int_requires_runtime_param_guard(
    expected: &PhpType,
    actual: &PhpType,
) -> bool {
    expected.codegen_repr() == PhpType::Int
        && actual.codegen_repr() == PhpType::TaggedScalar
}

/// Returns whether `ty` is php's `?int` (`int|null`), whatever storage the target gives it.
pub(crate) fn is_nullable_int(ty: &PhpType) -> bool {
    match ty {
        PhpType::Union(members) => {
            members.iter().any(|member| *member == PhpType::Int)
                && members
                    .iter()
                    .all(|member| matches!(member, PhpType::Int | PhpType::Void))
        }
        _ => false,
    }
}

/// How a declared parameter binds an argument whose type does not already match.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ParamBinding {
    /// PHP binds the argument unchanged; nothing to do.
    Identity,
    /// PHP coerces the argument exactly, with no notice and no failure mode. The argument is
    /// replaced by the equivalent explicit cast.
    Cast(CastType),
    /// PHP coerces a compile-time constant exactly. The argument is replaced by this literal.
    Const(ExprKind),
    /// The argument is a compile-time callable-name string bound to a `callable` parameter.
    /// The argument is replaced by the equivalent first-class callable expression.
    Callable(CallableTarget),
    /// PHP binds the argument but emits a runtime `Deprecated:` notice first. elephc has no
    /// runtime deprecation channel, so this is reported at compile time instead.
    Deprecated(String),
    /// PHP throws a `TypeError` when the call runs.
    TypeError(String),
    /// PHP would coerce, but only a runtime check can tell whether the conversion succeeds,
    /// and elephc cannot raise PHP's runtime notice/`TypeError` at a parameter boundary.
    NeedsRuntimeCheck(String),
    /// PHP binds the value unchanged and throws `TypeError` when it is not an instance of the
    /// declared class. EIR lowering inserts the runtime instance guard.
    ObjectGuard,
    /// No PHP parameter-binding rule applies; the caller keeps its existing diagnostic.
    Rejected,
}

/// Returns whether a property type can be written back from a boxed by-reference result.
///
/// Shared by the checker that authorizes a property place and the EIR rewrite that narrows
/// the reference cell back to the declared storage.
pub(crate) fn by_ref_property_writeback_supported(ty: &PhpType) -> bool {
    // Inspect the DECLARED type: a union or nullable property collapses to `Mixed` under
    // `codegen_repr`, but its write-back needs a runtime type dispatch the rewrite does not
    // implement. PHP's `array` contract is the one union the rewrite does support, because
    // both of its storage shapes stay inside the boxed container payload.
    if ty.is_php_array() {
        return true;
    }
    matches!(
        ty,
        PhpType::Object(_)
            | PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Str
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Union(_)
            | PhpType::Mixed
    )
}

/// Returns whether a declared by-reference parameter may receive a property of `property_ty`.
pub(crate) fn by_ref_property_storage_compatible(
    expected: &PhpType,
    property_ty: &PhpType,
) -> bool {
    if !by_ref_property_writeback_supported(expected)
        || !by_ref_property_writeback_supported(property_ty)
    {
        return false;
    }
    let expected = expected.codegen_repr();
    let property = property_ty.codegen_repr();
    match (expected, property) {
        (PhpType::Mixed, _) => true,
        (PhpType::Int, PhpType::Int)
        | (PhpType::Float, PhpType::Float)
        | (PhpType::Bool, PhpType::Bool)
        | (PhpType::Str, PhpType::Str)
        | (PhpType::Object(_), PhpType::Object(_)) => true,
        (PhpType::Array(_), PhpType::Array(_))
        | (PhpType::Array(_), PhpType::AssocArray { .. })
        | (PhpType::AssocArray { .. }, PhpType::Array(_))
        | (PhpType::AssocArray { .. }, PhpType::AssocArray { .. })
        | (PhpType::TaggedScalar, PhpType::TaggedScalar) => true,
        _ => false,
    }
}

/// Classifies how `expected` binds `arg` (whose inferred type is `actual`) under PHP's
/// default coercive parameter binding.
///
/// A `ParamBinding::Callable` result only says the string is *syntactically* a callable name;
/// whether it resolves to something invocable is decided by the caller against its own symbol
/// tables (the checker rejects an unresolvable name, so lowering never sees one).
///
/// Returns `ParamBinding::Rejected` whenever no PHP rule applies, leaving the caller's
/// existing type-mismatch diagnostic in charge.
pub(crate) fn classify_param_binding(
    expected: &PhpType,
    actual: &PhpType,
    arg: &Expr,
) -> ParamBinding {
    if expected == actual {
        return ParamBinding::Identity;
    }
    if *expected == PhpType::Callable {
        return classify_callable_string_binding(actual, arg);
    }
    // A union that names the declared class plus exactly one of `false`/`null` is accepted by
    // PHP at compile time; the call throws `TypeError` at runtime when the other alternative is
    // the one supplied. EIR lowering inserts that instance guard.
    if let PhpType::Object(expected_class) = expected {
        if let PhpType::Union(members) = actual {
            let expected_object = PhpType::Object(expected_class.clone());
            let has_expected = members.iter().filter(|member| **member == expected_object).count() == 1;
            let other = members
                .iter()
                .find(|member| **member != expected_object)
                .map(|member| matches!(member, PhpType::False | PhpType::Void));
            if has_expected && members.len() == 2 && other == Some(true) {
                return ParamBinding::ObjectGuard;
            }
        }
    }
    if weak_binding_selects_string_for_all_variants(expected, actual) {
        return ParamBinding::Cast(CastType::String);
    }
    match (expected.codegen_repr(), actual.codegen_repr()) {
        // `string $s` accepts every other scalar. `(string)` is total for int, float and
        // bool, produces no PHP notice, and elephc's cast already matches PHP byte for byte
        // (including `(string)false === ""` and float precision).
        (PhpType::Str, PhpType::Int | PhpType::Float | PhpType::Bool) => {
            ParamBinding::Cast(CastType::String)
        }
        // `bool $b` accepts every other scalar; `(bool)` is total and notice-free.
        (PhpType::Bool, PhpType::Float | PhpType::Str) => ParamBinding::Cast(CastType::Bool),
        // `int $i` / `float $f` from a float or string is partial: PHP emits `Deprecated:` on a
        // lossy conversion and throws `TypeError` on a non-numeric string, NaN, INF or an
        // out-of-range float. Only a compile-time constant can be decided here.
        (PhpType::Int, PhpType::Float | PhpType::Str)
        | (PhpType::Float, PhpType::Str) => classify_numeric_binding(expected, arg),
        _ => ParamBinding::Rejected,
    }
}

/// Returns whether a boxed gradual argument needs an active-tag check for a declared union.
///
/// The supported union members map directly to stable Mixed runtime tags, so the guard can
/// preserve the boxed ABI value unchanged after validation. Nominal objects, callables,
/// iterables, resources, and the literal `false` subtype need richer checks and deliberately
/// stay on their existing paths.
pub(crate) fn gradual_union_requires_runtime_param_guard(
    expected: &PhpType,
    actual: &PhpType,
) -> bool {
    if expected == actual
        || expected.codegen_repr() != PhpType::Mixed
        || actual.codegen_repr() != PhpType::Mixed
    {
        return false;
    }
    let PhpType::Union(members) = expected else {
        return false;
    };
    !members.is_empty()
        && members.iter().all(|member| {
            matches!(
                member,
                PhpType::Int
                    | PhpType::Float
                    | PhpType::Str
                    | PhpType::Bool
                    | PhpType::Void
                    | PhpType::Array(_)
                    | PhpType::AssocArray { .. }
            )
        })
}

/// Returns whether weak PHP binding selects `string` for every possible source variant.
///
/// Exact scalar arms and PHP's numeric-preference rules take precedence over string coercion.
/// Restricting this fallback to unions without `int`, `float`, or `bool` therefore covers total
/// scalar conversion without guessing about lossy numeric union selection. A nullable source is
/// accepted only when the target is nullable too; lowering then preserves null and casts only the
/// non-null scalar branch.
fn weak_binding_selects_string_for_all_variants(
    expected: &PhpType,
    actual: &PhpType,
) -> bool {
    let (target_accepts_string, target_accepts_null) = match expected {
        PhpType::Str => (true, false),
        PhpType::Union(members) => {
            let mut has_string = false;
            let mut has_null = false;
            for member in members {
                match member {
                    PhpType::Str => has_string = true,
                    PhpType::Void => has_null = true,
                    PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False => {
                        return false;
                    }
                    _ => {}
                }
            }
            (has_string, has_null)
        }
        _ => (false, false),
    };
    if !target_accepts_string {
        return false;
    }
    match actual {
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False => true,
        PhpType::Union(members) => !members.is_empty()
            && members.iter().all(|member| match member {
                PhpType::Str
                | PhpType::Int
                | PhpType::Float
                | PhpType::Bool
                | PhpType::False => true,
                PhpType::Void => target_accepts_null,
                _ => false,
            }),
        _ => false,
    }
}

/// The four PHP scalar type declarations `declare(strict_types=1)` compares by identity.
///
/// `false` collapses into `Bool` because it is a subtype of `bool`, not a separate scalar for
/// binding purposes. Every non-scalar declaration — objects, arrays, `iterable`, `callable`,
/// `mixed`, unions, elephc's `Mixed`/`Pointer`/`Resource` — is deliberately absent: strict mode
/// changes nothing about how those bind, so they must keep their existing acceptance rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrictScalar {
    Int,
    Float,
    Str,
    Bool,
}

impl StrictScalar {
    /// Returns the PHP spelling used in a `TypeError` message and in a cast suggestion.
    fn php_name(self) -> &'static str {
        match self {
            StrictScalar::Int => "int",
            StrictScalar::Float => "float",
            StrictScalar::Str => "string",
            StrictScalar::Bool => "bool",
        }
    }
}

/// Classifies a declared type as one of PHP's scalar type declarations, or `None` when strict
/// mode has nothing to say about it.
///
/// The *declared* type is inspected rather than `codegen_repr()`, which would flatten a union to
/// `Mixed` and a resource to `Int` and so invent scalar identities strict mode must not judge.
fn strict_scalar_kind(ty: &PhpType) -> Option<StrictScalar> {
    match ty {
        PhpType::Int => Some(StrictScalar::Int),
        PhpType::Float => Some(StrictScalar::Float),
        PhpType::Str => Some(StrictScalar::Str),
        PhpType::Bool | PhpType::False => Some(StrictScalar::Bool),
        _ => None,
    }
}

/// Reports why `declare(strict_types=1)` forbids binding `actual` to a declared `expected`
/// parameter, or `None` when PHP performs the binding in strict mode too.
///
/// PHP keeps exactly one implicit conversion under the directive — widening `int` to a declared
/// `float` — and throws `TypeError` for every other scalar pair, including the `bool`→`int`,
/// `int`→`string` and numeric-string→`int` conversions its coercive mode performs silently.
/// Verified against PHP 8.4.20 for the full 4x4 scalar matrix.
///
/// Returns `None` for any pair involving a non-scalar declared type: strict mode does not change
/// how those bind, so the caller's normal compatibility rules stay in charge.
pub(crate) fn strict_param_binding_rejection(
    expected: &PhpType,
    actual: &PhpType,
) -> Option<String> {
    let expected_kind = strict_scalar_kind(expected)?;
    let actual_kind = strict_scalar_kind(actual)?;
    if expected_kind == actual_kind {
        return None;
    }
    if expected_kind == StrictScalar::Float && actual_kind == StrictScalar::Int {
        return None;
    }
    Some(format!(
        "`declare(strict_types=1)` is active in this file, so PHP performs no conversion here \
         and throws `TypeError: ... must be of type {}, {} given`; add an explicit `({})` cast \
         at the call site",
        expected_kind.php_name(),
        actual_kind.php_name(),
        expected_kind.php_name()
    ))
}

/// Classifies a string argument bound to a declared `callable` parameter.
///
/// PHP accepts `"function"` and `"Class::method"` strings as callables and resolves them when
/// the call runs. elephc resolves callables statically, so only a compile-time-known string is
/// bindable; anything else gets a named diagnostic instead of storage that cannot be invoked.
fn classify_callable_string_binding(actual: &PhpType, arg: &Expr) -> ParamBinding {
    if actual.codegen_repr() != PhpType::Str {
        return ParamBinding::Rejected;
    }
    let ExprKind::StringLiteral(name) = &arg.kind else {
        return ParamBinding::NeedsRuntimeCheck(
            "a callable string must be a compile-time constant here, because elephc resolves \
             callables statically; pass a first-class callable (`strtoupper(...)`), a closure, \
             or a literal function name"
                .to_string(),
        );
    };
    match callable_target_from_name(name) {
        Some(target) => ParamBinding::Callable(target),
        None => ParamBinding::TypeError(format!(
            "\"{}\" is not a valid callable name",
            name.escape_debug()
        )),
    }
}

/// Parses a PHP callable string into the equivalent first-class callable target.
///
/// `"Class::method"` becomes a static-method target and everything else a plain function
/// target, matching the split `crate::ir_lower` already performs for `call_user_func`
/// string callbacks. A leading `\` is dropped because a callable string is always
/// fully qualified in PHP. Returns `None` for a string that cannot name anything.
pub(crate) fn callable_target_from_name(name: &str) -> Option<CallableTarget> {
    let name = name.trim_start_matches('\\');
    if name.is_empty() {
        return None;
    }
    if let Some((class_name, method)) = name.rsplit_once("::") {
        let class_name = class_name.trim_start_matches('\\');
        if class_name.is_empty() || method.is_empty() || method.contains("::") {
            return None;
        }
        return Some(CallableTarget::StaticMethod {
            receiver: StaticReceiver::Named(Name::unqualified(class_name)),
            method: method.to_string(),
        });
    }
    Some(CallableTarget::Function(Name::unqualified(name)))
}

/// Classifies a float or string argument bound to an `int`/`float` parameter.
///
/// Only literals are decided: PHP's failure modes for this direction are a runtime
/// `Deprecated:` notice (lossy conversion) and a runtime `TypeError` (non-numeric string, NaN,
/// INF, out-of-range float), neither of which elephc can raise at a parameter boundary.
fn classify_numeric_binding(expected: &PhpType, arg: &Expr) -> ParamBinding {
    match (&expected.codegen_repr(), &arg.kind) {
        (PhpType::Int, ExprKind::FloatLiteral(value)) => const_float_to_int(*value),
        (PhpType::Int, ExprKind::StringLiteral(text)) => const_string_to_int(text),
        (PhpType::Float, ExprKind::StringLiteral(text)) => const_string_to_float(text),
        _ => ParamBinding::NeedsRuntimeCheck(
            "PHP coerces this at run time and signals failure with a `Deprecated:` notice or a \
             `TypeError`, which elephc cannot raise at a parameter boundary; add an explicit \
             cast at the call site"
                .to_string(),
        ),
    }
}

/// Applies PHP's float-to-int parameter coercion to a literal.
///
/// An integral, in-range float binds silently; a fractional one binds after PHP's
/// `Implicit conversion ... loses precision` deprecation; NaN, INF and out-of-range values
/// throw `TypeError`.
fn const_float_to_int(value: f64) -> ParamBinding {
    if !value.is_finite() || value < -(2f64.powi(63)) || value >= 2f64.powi(63) {
        return ParamBinding::TypeError(format!(
            "PHP throws `TypeError` for the float {} at an `int` parameter",
            php_float_text(value)
        ));
    }
    if value.fract() != 0.0 {
        return ParamBinding::Deprecated(format!(
            "PHP emits `Deprecated: Implicit conversion from float {} to int loses precision` \
             and passes {}",
            php_float_text(value),
            value.trunc() as i64
        ));
    }
    ParamBinding::Const(ExprKind::IntLiteral(value as i64))
}

/// Applies PHP's string-to-int parameter coercion to a literal.
///
/// A fully numeric integral string binds silently, a fully numeric fractional string binds
/// after PHP's float-string deprecation, and everything else — including a leading-numeric
/// string such as `"42abc"` — throws `TypeError`.
fn const_string_to_int(text: &str) -> ParamBinding {
    let Some(scan) = scan_php_numeric_string(text) else {
        return ParamBinding::TypeError(format!(
            "PHP throws `TypeError` for the non-numeric string \"{}\" at an `int` parameter",
            text.escape_debug()
        ));
    };
    if !scan.is_float {
        return match scan.text.parse::<i64>() {
            Ok(value) => ParamBinding::Const(ExprKind::IntLiteral(value)),
            // An integral spelling that overflows `i64` is a float to PHP, so it reaches the
            // same out-of-range `TypeError` as an oversized float literal.
            Err(_) => ParamBinding::TypeError(format!(
                "PHP throws `TypeError` for the out-of-range numeric string \"{}\" at an `int` \
                 parameter",
                text.escape_debug()
            )),
        };
    }
    let Ok(value) = scan.text.parse::<f64>() else {
        return ParamBinding::TypeError(format!(
            "PHP throws `TypeError` for the numeric string \"{}\" at an `int` parameter",
            text.escape_debug()
        ));
    };
    if value.fract() == 0.0 && value.is_finite() && value.abs() < 2f64.powi(63) {
        return ParamBinding::Const(ExprKind::IntLiteral(value as i64));
    }
    ParamBinding::Deprecated(format!(
        "PHP emits `Deprecated: Implicit conversion from float-string \"{}\" to int loses \
         precision`",
        text.escape_debug()
    ))
}

/// Applies PHP's string-to-float parameter coercion to a literal.
///
/// A fully numeric string binds silently; every other string throws `TypeError`.
fn const_string_to_float(text: &str) -> ParamBinding {
    match scan_php_numeric_string(text).and_then(|scan| scan.text.parse::<f64>().ok()) {
        Some(value) => ParamBinding::Const(ExprKind::FloatLiteral(value)),
        None => ParamBinding::TypeError(format!(
            "PHP throws `TypeError` for the non-numeric string \"{}\" at a `float` parameter",
            text.escape_debug()
        )),
    }
}

/// Scans a *fully* numeric string using PHP's numeric-string grammar, returning `None` when
/// trailing non-whitespace bytes remain.
///
/// Parameter binding only ever accepts fully numeric strings, so a leading-numeric string
/// like `"42abc"` returns `None` here and reaches PHP's `TypeError`. The grammar itself lives
/// in [`crate::numeric_string`], the single compile-time twin of the runtime
/// `__rt_php_num_scan` helper.
fn scan_php_numeric_string(value: &str) -> Option<NumericScan<'_>> {
    let scan = scan_numeric_prefix(value)?;
    scan.is_fully_numeric().then_some(scan)
}

/// Renders a float the way PHP spells it inside a diagnostic (`5.5`, `INF`, `NAN`).
fn php_float_text(value: f64) -> String {
    if value.is_nan() {
        return "NAN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() { "-INF" } else { "INF" }.to_string();
    }
    format!("{}", value)
}

/// Rewrites an argument whose parameter binding is decidable from its literal spelling alone,
/// before EIR lowering evaluates it.
///
/// This covers the two bindings that must not lower the original argument at all: a
/// callable-name string becomes the equivalent first-class callable, and a constant bound to
/// `int`/`float` becomes the already-coerced literal. Scalar `(string)`/`(bool)` bindings are
/// applied to the lowered *value* instead (see `coerce_scalar_arg_to_param_storage`), because
/// they must also fire for runtime values whose type is only known after lowering.
///
/// Returns `None` when the argument needs no pre-lowering rewrite. The checker has already
/// rejected every binding this can produce that would not resolve, so lowering can apply the
/// rewrite unconditionally.
pub(crate) fn rewrite_literal_param_binding(expected: &PhpType, arg: &Expr) -> Option<Expr> {
    let actual = literal_php_type(&arg.kind)?;
    match classify_param_binding(expected, &actual, arg) {
        ParamBinding::Const(kind) => Some(Expr::new(kind, arg.span)),
        ParamBinding::Callable(target) => {
            Some(Expr::new(ExprKind::FirstClassCallable(target), arg.span))
        }
        _ => None,
    }
}

/// Returns the PHP type of an argument whose type is fixed by its literal spelling.
///
/// Only the literals parameter binding can decide without inference are recognized; anything
/// else returns `None` so the caller falls back to a value-level binding after lowering.
fn literal_php_type(kind: &ExprKind) -> Option<PhpType> {
    match kind {
        ExprKind::StringLiteral(_) => Some(PhpType::Str),
        ExprKind::FloatLiteral(_) => Some(PhpType::Float),
        ExprKind::IntLiteral(_) => Some(PhpType::Int),
        ExprKind::BoolLiteral(_) => Some(PhpType::Bool),
        _ => None,
    }
}

/// Returns the explicit cast that implements a *value-level* parameter binding.
///
/// `actual` is the lowered argument's codegen type, so this fires for runtime values as well
/// as literals. Returns `None` when no total scalar coercion applies. An object conversion is
/// returned only for a declared string arm; the checker has already proven the concrete object
/// implements `Stringable` before lowering consumes this helper.
///
/// The classifier is shared with the checker so the two cannot disagree; the placeholder
/// argument is inert because only the value-independent `ParamBinding::Cast` outcome is kept,
/// and every expression-sensitive outcome maps to `None` here.
pub(crate) fn scalar_param_cast(expected: &PhpType, actual: &PhpType) -> Option<CastType> {
    if matches!(actual.codegen_repr(), PhpType::Object(_))
        && param_accepts_weak_string_coercion(expected)
        && !union_admits_object_as_is(expected, actual)
    {
        // The checker only admits this path after proving that the concrete object implements
        // `Stringable`. Lowering deliberately consumes that proof instead of duplicating class
        // metadata, which is not available at this stage.
        return Some(CastType::String);
    }
    let placeholder = Expr::new(ExprKind::Null, crate::span::Span::dummy());
    match classify_param_binding(expected, actual, &placeholder) {
        ParamBinding::Cast(target) => Some(target),
        _ => None,
    }
}

/// Whether a union holds the object unchanged, so PHP never reaches its `string` arm.
///
/// PHP coerces an argument only when no member accepts it as it is. Doctrine's
/// `ParameterTypeInferer::inferType(): ParameterType|ArrayParameterType|int|string` returns
/// `ArrayParameterType::INTEGER`; the `string` member made every return a string cast, and the
/// enum case died with `could not be converted to string` on every symfony/demo query with an
/// `IN (:ids)` list.
fn union_admits_object_as_is(expected: &PhpType, actual: &PhpType) -> bool {
    let PhpType::Object(actual_class) = actual.codegen_repr() else {
        return false;
    };
    let actual_class = actual_class.trim_start_matches('\\');
    let PhpType::Union(members) = expected else {
        return false;
    };
    members.iter().any(|member| match member {
        PhpType::Mixed => true,
        PhpType::Object(name) => {
            let name = name.trim_start_matches('\\');
            name.eq_ignore_ascii_case(actual_class) || name.eq_ignore_ascii_case("object")
        }
        _ => false,
    })
}

/// Returns whether a declared parameter contains a `string` arm that PHP may select through
/// weak object-to-string parameter coercion.
///
/// This inspects the source declaration rather than `codegen_repr()`: union storage collapses
/// to `Mixed`, but PHP still selects its `string` member when a `Stringable` object is passed
/// from a non-strict call site.
pub(crate) fn param_accepts_weak_string_coercion(expected: &PhpType) -> bool {
    match expected {
        PhpType::Str => true,
        PhpType::Union(members) => members.iter().any(param_accepts_weak_string_coercion),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a string-literal argument expression for the binding classifiers.
    fn string_arg(text: &str) -> Expr {
        Expr::new(
            ExprKind::StringLiteral(text.to_string()),
            crate::span::Span::dummy(),
        )
    }

    /// Verifies the numeric-string scanner matches PHP's `is_numeric()` for the spellings
    /// parameter binding depends on, including trailing whitespace and rejected garbage.
    #[test]
    fn numeric_string_scan_matches_php_is_numeric() {
        assert!(scan_php_numeric_string("42").is_some());
        assert!(scan_php_numeric_string(" 42 ").is_some());
        assert!(scan_php_numeric_string("-4.5").is_some());
        assert!(scan_php_numeric_string("1e3").is_some());
        assert!(scan_php_numeric_string("42abc").is_none());
        assert!(scan_php_numeric_string("abc").is_none());
        assert!(scan_php_numeric_string("0x1A").is_none());
        assert!(scan_php_numeric_string("1_000").is_none());
        assert!(scan_php_numeric_string("").is_none());
    }

    /// Verifies an integral numeric string binds to `int` as a literal while a fractional one
    /// is reported as PHP's deprecation and a non-numeric one as PHP's `TypeError`.
    #[test]
    fn string_to_int_binding_follows_php() {
        assert_eq!(
            const_string_to_int("42"),
            ParamBinding::Const(ExprKind::IntLiteral(42))
        );
        assert_eq!(
            const_string_to_int(" 42 "),
            ParamBinding::Const(ExprKind::IntLiteral(42))
        );
        assert!(matches!(
            const_string_to_int("4.5"),
            ParamBinding::Deprecated(_)
        ));
        assert!(matches!(
            const_string_to_int("42abc"),
            ParamBinding::TypeError(_)
        ));
    }

    /// Verifies float literals bind to `int` only when they are integral and in range.
    #[test]
    fn float_to_int_binding_follows_php() {
        assert_eq!(
            const_float_to_int(5.0),
            ParamBinding::Const(ExprKind::IntLiteral(5))
        );
        assert!(matches!(const_float_to_int(5.5), ParamBinding::Deprecated(_)));
        assert!(matches!(
            const_float_to_int(f64::NAN),
            ParamBinding::TypeError(_)
        ));
        assert!(matches!(
            const_float_to_int(1e20),
            ParamBinding::TypeError(_)
        ));
    }

    /// Verifies `"Class::method"` splits into a static-method target and a plain name into a
    /// function target, with a leading `\` dropped.
    #[test]
    fn callable_strings_split_into_targets() {
        assert!(matches!(
            callable_target_from_name("strtoupper"),
            Some(CallableTarget::Function(_))
        ));
        assert!(matches!(
            callable_target_from_name("\\strtoupper"),
            Some(CallableTarget::Function(_))
        ));
        assert!(matches!(
            callable_target_from_name("Formatter::wrap"),
            Some(CallableTarget::StaticMethod { .. })
        ));
        assert!(callable_target_from_name("").is_none());
        assert!(callable_target_from_name("::wrap").is_none());
    }

    /// Verifies a non-literal callable argument is reported as needing a runtime check rather
    /// than being silently accepted into a callable slot that cannot be invoked.
    #[test]
    fn runtime_callable_string_is_not_bindable() {
        let arg = Expr::new(
            ExprKind::Variable("f".to_string()),
            crate::span::Span::dummy(),
        );
        let binding = classify_param_binding(&PhpType::Callable, &PhpType::Str, &arg);
        assert!(matches!(binding, ParamBinding::NeedsRuntimeCheck(_)));
    }

    /// Verifies `declare(strict_types=1)` keeps only the `int`→`float` widening across the full
    /// 4x4 scalar matrix, matching PHP 8.4.20's accept/reject table.
    #[test]
    fn strict_types_keeps_only_the_int_to_float_widening() {
        let scalars = [
            PhpType::Int,
            PhpType::Float,
            PhpType::Str,
            PhpType::Bool,
        ];
        for expected in &scalars {
            for actual in &scalars {
                let accepted = strict_param_binding_rejection(expected, actual).is_none();
                let should_accept = expected == actual
                    || (*expected == PhpType::Float && *actual == PhpType::Int);
                assert_eq!(
                    accepted, should_accept,
                    "strict binding of {:?} into {:?}",
                    actual, expected
                );
            }
        }
    }

    /// Verifies the strict rejection stays out of every non-scalar declaration, so `mixed`,
    /// arrays, unions, `callable` and class types keep the acceptance rules they already had.
    #[test]
    fn strict_types_ignores_non_scalar_declarations() {
        assert!(strict_param_binding_rejection(&PhpType::Mixed, &PhpType::Int).is_none());
        assert!(strict_param_binding_rejection(&PhpType::Int, &PhpType::Mixed).is_none());
        assert!(strict_param_binding_rejection(&PhpType::Callable, &PhpType::Str).is_none());
        assert!(strict_param_binding_rejection(
            &PhpType::Array(Box::new(PhpType::Int)),
            &PhpType::Str
        )
        .is_none());
        assert!(strict_param_binding_rejection(
            &PhpType::Union(vec![PhpType::Int, PhpType::Str]),
            &PhpType::Bool
        )
        .is_none());
        // `false` is a subtype of `bool`, not a distinct scalar for binding purposes.
        assert!(strict_param_binding_rejection(&PhpType::Bool, &PhpType::False).is_none());
    }

    /// Verifies the strict diagnostic names PHP's `TypeError` types and suggests the cast that
    /// makes the call legal, so the message is actionable rather than just a refusal.
    #[test]
    fn strict_types_diagnostic_names_the_php_type_error() {
        let detail = strict_param_binding_rejection(&PhpType::Int, &PhpType::Str)
            .expect("string into int is rejected under strict_types");
        assert!(detail.contains("declare(strict_types=1)"), "{}", detail);
        assert!(detail.contains("must be of type int, string given"), "{}", detail);
        assert!(detail.contains("`(int)` cast"), "{}", detail);
    }

    /// Verifies only an exact `Class|false` / `Class|null` union earns the runtime object guard.
    #[test]
    fn object_union_binding_requires_the_declared_class_plus_false_or_null() {
        let arg = Expr::new(
            ExprKind::Variable("value".to_string()),
            crate::span::Span::dummy(),
        );
        let class = PhpType::Object("Marker".to_string());
        assert_eq!(
            classify_param_binding(
                &class,
                &PhpType::Union(vec![class.clone(), PhpType::False]),
                &arg,
            ),
            ParamBinding::ObjectGuard
        );
        assert_eq!(
            classify_param_binding(
                &class,
                &PhpType::Union(vec![PhpType::Void, class.clone()]),
                &arg,
            ),
            ParamBinding::ObjectGuard
        );
        for rejected in [
            PhpType::Union(vec![class.clone(), PhpType::Int]),
            PhpType::Union(vec![PhpType::Object("Other".to_string()), PhpType::False]),
            PhpType::Union(vec![class.clone(), PhpType::False, PhpType::Void]),
        ] {
            assert!(
                matches!(
                    classify_param_binding(&class, &rejected, &arg),
                    ParamBinding::Rejected
                ),
                "{rejected:?}"
            );
        }
    }

    /// Verifies a scalar bound to `string` becomes an explicit `(string)` cast, which is the
    /// conversion elephc already implements PHP-exactly.
    #[test]
    fn scalar_to_string_binding_uses_the_cast() {
        let arg = Expr::new(ExprKind::IntLiteral(42), crate::span::Span::dummy());
        assert_eq!(
            classify_param_binding(&PhpType::Str, &PhpType::Int, &arg),
            ParamBinding::Cast(CastType::String)
        );
        assert_eq!(
            classify_param_binding(&PhpType::Bool, &PhpType::Str, &string_arg("a")),
            ParamBinding::Cast(CastType::Bool)
        );
    }

    /// Verifies a union with `string` as its only scalar arm performs total weak conversion.
    #[test]
    fn scalar_to_sole_string_union_binding_uses_the_cast() {
        let expected = PhpType::Union(vec![
            PhpType::Str,
            PhpType::Array(Box::new(PhpType::Mixed)),
            PhpType::Void,
        ]);
        let arg = Expr::new(ExprKind::IntLiteral(42), crate::span::Span::dummy());
        assert_eq!(
            classify_param_binding(&expected, &PhpType::Int, &arg),
            ParamBinding::Cast(CastType::String)
        );
        let numeric_union = PhpType::Union(vec![PhpType::Int, PhpType::Str]);
        assert_eq!(
            classify_param_binding(&numeric_union, &PhpType::Bool, &arg),
            ParamBinding::Rejected
        );
        let nullable_scalar = PhpType::Union(vec![PhpType::Int, PhpType::Void]);
        assert_eq!(
            classify_param_binding(&expected, &nullable_scalar, &arg),
            ParamBinding::Cast(CastType::String)
        );
        assert_eq!(
            classify_param_binding(&PhpType::Str, &nullable_scalar, &arg),
            ParamBinding::Rejected
        );
    }

    /// Verifies lowering retains the declared union shape when selecting its weakly coercive
    /// `string` member for an object already proven `Stringable` by the checker.
    #[test]
    fn stringable_object_binding_selects_string_union_member() {
        let expected = PhpType::Union(vec![PhpType::Str, PhpType::Iterable]);
        let actual = PhpType::Object("Label".to_string());
        assert!(param_accepts_weak_string_coercion(&expected));
        assert_eq!(
            scalar_param_cast(&expected, &actual),
            Some(CastType::String)
        );
        assert!(!param_accepts_weak_string_coercion(&PhpType::Iterable));
    }
}
