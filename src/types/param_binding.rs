//! Purpose:
//! Owns the PHP parameter-binding rules that let a declared parameter accept an argument
//! of a different type: coercive scalar binding (`string $s` accepting `42`), callable-name
//! strings (`callable $f` accepting `"strtoupper"`), and the `declare(strict_types=1)` mode
//! that switches every one of those conversions off.
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
//! - Only *total* coercions (every value of the source type converts, with no PHP diagnostic)
//!   run on runtime values. Partial coercions (`int $i` from a float or string) are limited to
//!   compile-time constants, because PHP signals their failure modes with a runtime
//!   `Deprecated:` notice or a `TypeError` and elephc has neither channel at a call boundary.

use crate::parser::ast::{CallableTarget, CastType, Expr, ExprKind, StaticReceiver};
use crate::names::Name;
use crate::types::PhpType;

/// Identifies by-value object arguments that require a runtime class/interface guard.
pub(crate) fn requires_object_argument_guard(expected: &PhpType, actual: &PhpType) -> bool {
    matches!(expected, PhpType::Object(_)) && *actual == PhpType::Mixed
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
    /// No PHP parameter-binding rule applies; the caller keeps its existing diagnostic.
    Rejected,
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
    if let Some(inner @ PhpType::Int) = nullable_scalar_member(expected) {
        if *actual == PhpType::Void {
            return ParamBinding::Identity;
        }
        return classify_param_binding(inner, actual, arg);
    }
    if *expected == PhpType::Callable {
        return classify_callable_string_binding(actual, arg);
    }
    match (expected.codegen_repr(), actual.codegen_repr()) {
        (PhpType::Int, PhpType::Bool) => ParamBinding::Cast(CastType::Int),
        (PhpType::Bool, PhpType::Int) => ParamBinding::Cast(CastType::Bool),
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

/// Extracts the scalar member of a nullable declaration without treating general unions as scalars.
fn nullable_scalar_member(ty: &PhpType) -> Option<&PhpType> {
    let PhpType::Union(members) = ty else { return None; };
    if members.len() != 2 || !members.contains(&PhpType::Void) {
        return None;
    }
    members.iter().find(|member| matches!(member,
        PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool | PhpType::False))
}

/// Returns the scalar kind compared by strict parameter binding, unwrapping nullability.
/// Uses declared types, not codegen representations, so resources are not mistaken for integers.
fn strict_scalar_kind(ty: &PhpType) -> Option<StrictScalar> {
    let ty = nullable_scalar_member(ty).unwrap_or(ty);
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

/// A string that satisfies PHP's `is_numeric()` grammar, split into its numeric text and
/// whether that text spells a float.
struct NumericString<'a> {
    /// The numeric run with PHP whitespace stripped from both ends.
    text: &'a str,
    /// True when the run contains a decimal point or a consumed exponent (PHP `IS_DOUBLE`).
    is_float: bool,
}

/// Scans a *fully* numeric string using PHP's numeric-string grammar, returning `None` when
/// trailing non-whitespace bytes remain.
///
/// This is the compile-time twin of the runtime `__rt_php_num_scan` helper
/// (`crate::codegen_support::runtime::strings::php_num_scan`) restricted to
/// `is_numeric() === true`: leading and trailing PHP whitespace are allowed, an optional sign,
/// a mantissa with at least one digit, and an exponent only when a digit follows it. Hex,
/// underscore separators, `INF` and `NAN` are not part of the grammar. Parameter binding only
/// ever accepts fully numeric strings, so a leading-numeric string like `"42abc"` returns
/// `None` here and reaches PHP's `TypeError`.
fn scan_php_numeric_string(value: &str) -> Option<NumericString<'_>> {
    let bytes = value.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && is_php_whitespace(bytes[idx]) {
        idx += 1;
    }

    let start = idx;
    if idx < bytes.len() && matches!(bytes[idx], b'+' | b'-') {
        idx += 1;
    }

    let mut digits = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
        digits += 1;
    }

    let mut is_float = false;
    if idx < bytes.len() && bytes[idx] == b'.' {
        let mut probe = idx + 1;
        while probe < bytes.len() && bytes[probe].is_ascii_digit() {
            probe += 1;
            digits += 1;
        }
        if digits > 0 {
            idx = probe;
            is_float = true;
        }
    }
    if digits == 0 {
        return None;
    }

    if idx < bytes.len() && matches!(bytes[idx], b'e' | b'E') {
        let mut probe = idx + 1;
        if probe < bytes.len() && matches!(bytes[probe], b'+' | b'-') {
            probe += 1;
        }
        let exponent_start = probe;
        while probe < bytes.len() && bytes[probe].is_ascii_digit() {
            probe += 1;
        }
        if probe > exponent_start {
            idx = probe;
            is_float = true;
        }
    }

    let end = idx;
    while idx < bytes.len() && is_php_whitespace(bytes[idx]) {
        idx += 1;
    }
    if idx != bytes.len() {
        return None;
    }

    Some(NumericString {
        text: &value[start..end],
        is_float,
    })
}

/// Returns true for the bytes PHP's numeric-string grammar treats as leading/trailing space.
fn is_php_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
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

/// Returns the explicit cast that implements a *value-level* scalar parameter binding.
///
/// `actual` is the lowered argument's codegen type, so this fires for runtime values as well
/// as literals. Returns `None` when no total scalar coercion applies.
///
/// The classifier is shared with the checker so the two cannot disagree; the placeholder
/// argument is inert because only the value-independent `ParamBinding::Cast` outcome is kept,
/// and every expression-sensitive outcome maps to `None` here.
pub(crate) fn scalar_param_cast(expected: &PhpType, actual: &PhpType) -> Option<CastType> {
    let placeholder = Expr::new(ExprKind::Null, crate::span::Span::dummy());
    match classify_param_binding(expected, actual, &placeholder) {
        ParamBinding::Cast(target) => Some(target),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nullable integer binding retains null and applies the same weak/strict scalar rules.
    #[test]
    fn nullable_integer_binding_keeps_null_and_scalar_policy() {
        let nullable = PhpType::Union(vec![PhpType::Int, PhpType::Void]);
        let argument = Expr::new(ExprKind::FloatLiteral(23.0), crate::span::Span::dummy());
        assert!(matches!(classify_param_binding(&nullable, &PhpType::Float, &argument),
            ParamBinding::Const(ExprKind::IntLiteral(23))));
        let null = Expr::new(ExprKind::Null, crate::span::Span::dummy());
        assert!(matches!(classify_param_binding(&nullable, &PhpType::Void, &null), ParamBinding::Identity));
        assert!(strict_param_binding_rejection(&nullable, &PhpType::Float).is_some());
        assert!(strict_param_binding_rejection(&nullable, &PhpType::Void).is_none());
        assert!(strict_param_binding_rejection(&nullable, &PhpType::Int).is_none());
    }

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
}
