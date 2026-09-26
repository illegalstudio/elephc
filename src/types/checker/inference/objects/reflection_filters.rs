//! Purpose:
//! Rejects `ReflectionX::getAttributes()` filter arguments the synthesized body cannot honour.
//!
//! Called from:
//! - `crate::types::checker::inference::objects::methods` before instance method inference, for
//!   both a known receiver class and a `mixed` receiver dispatched on the runtime class id.
//!
//! Key details:
//! - Only the `$flags` argument is refused; the `$name` filter is implemented.
//! - The argument list is read in every spelling PHP allows, because each one that slipped past
//!   turned a compile error into a silently wrong answer.
//! - Static associative spreads are expanded through the SHARED expander the call planner uses,
//!   rather than being treated as opaque. Refusing them here made
//!   `getAttributes(...['name' => M::class, 'flags' => 0])` a compile error for a list the rest
//!   of the compiler normalizes into named arguments. A spread that is not a static associative
//!   literal survives the expansion and still reads as `Hidden`.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::call_args::plan_call_args;
use crate::types::checker::Checker;
use crate::types::FunctionSig;

/// The synthesized Reflection classes that carry a `getAttributes()` method over `__attrs`.
const REFLECTION_ATTRIBUTE_OWNERS: [&str; 10] = [
    "ReflectionClass",
    "ReflectionObject",
    "ReflectionEnum",
    "ReflectionFunction",
    "ReflectionMethod",
    "ReflectionProperty",
    "ReflectionParameter",
    "ReflectionClassConstant",
    "ReflectionEnumUnitCase",
    "ReflectionEnumBackedCase",
];

/// What the call site says about `getAttributes()`'s `$flags`, as far as the AST can show it.
enum FilterFlags {
    /// The call passes no `$flags` at all, so PHP's default `0` applies.
    Absent,
    /// The expression passed for `$flags`.
    Given(Expr),
    /// A spread hides the argument list: `getAttributes(...$args)` is one AST argument whose
    /// contents are a runtime array, so nothing here can tell `$flags` from absent.
    Hidden,
    /// The shared planner rejected the call; ordinary call validation owns its diagnostic.
    Invalid,
}

/// Returns the `$name` and `$flags` arguments of a `getAttributes()` call.
///
/// The shared call plan puts named values into signature order and expands static associative
/// spreads. A dynamic spread remains `Hidden` because it can supply either filter at run time.
fn get_attributes_filter_arguments(
    sig: &FunctionSig,
    args: &[Expr],
    span: Span,
) -> (Option<Expr>, FilterFlags) {
    let Ok(plan) = plan_call_args(sig, args, span, false, false) else {
        // The ordinary call checker owns PHP's diagnostics. This guard only inspects a valid plan.
        return (None, FilterFlags::Invalid);
    };
    if plan.has_spread_args() {
        // The planner passes a spread through without resolving slots, so a spread of an array
        // LITERAL is read here: its elements are the arguments, by position or by name. Treating
        // `...[Other::class]` as able to carry `$flags` refused valid PHP, while
        // `...[Marker::class, 2]` must still be read as a non-zero flag. Only a spread of a
        // runtime array is really hidden.
        return match args {
            [Expr { kind: ExprKind::Spread(inner), .. }] => literal_spread_filter_arguments(inner),
            _ => (None, FilterFlags::Hidden),
        };
    }
    let normalized = plan.normalized_args();
    (
        normalized.first().cloned(),
        normalized
            .get(1)
            .cloned()
            .map(FilterFlags::Given)
            .unwrap_or(FilterFlags::Absent),
    )
}

/// Reads `$name` and `$flags` from the sole argument `...[<plain values>]`; any other spread
/// source cannot be read at compile time and hides both.
///
/// Only an indexed literal without nested spreads is read: its element index IS the argument
/// index. A nested spread (`...[...$args]`) breaks that, and can carry a runtime flag, so it hides
/// both. An associative literal never reaches here with all-literal keys, because the call planner
/// expands those into named arguments first; any other one stays hidden.
fn literal_spread_filter_arguments(spread: &Expr) -> (Option<Expr>, FilterFlags) {
    let ExprKind::ArrayLiteral(items) = &spread.kind else {
        return (None, FilterFlags::Hidden);
    };
    if items.iter().any(|item| matches!(item.kind, ExprKind::Spread(_))) {
        return (None, FilterFlags::Hidden);
    }
    (
        items.first().cloned(),
        items.get(1).cloned().map_or(FilterFlags::Absent, FilterFlags::Given),
    )
}

impl Checker {
    /// Rejects a `getAttributes()` call whose `$flags` argument is not a compile-time zero.
    ///
    /// PHP's only documented flag is `ReflectionAttribute::IS_INSTANCEOF`, which widens the
    /// `$name` filter to subclasses and implemented interfaces. Deciding that needs a subclass
    /// test on the ATTRIBUTE's own class name, which the synthesized body only has as a runtime
    /// string, and no name-keyed hierarchy query accepts it in AOT mode: `is_subclass_of()`
    /// accepts literal class names but answers `false` for a runtime-only name, while
    /// `class_parents()`, `class_implements()` and `class_exists()` reject non-literal names (#1113).
    ///
    /// Honouring the flag by exact name instead would return a SUBSET of what PHP returns, with
    /// no diagnostic. Before the `$name` filter existed this call was a compile error anyway
    /// (`getAttributes` declared no parameters at all), so refusing the flag keeps a loud failure
    /// loud instead of trading it for a quiet wrong answer.
    ///
    /// `class_name` is `None` when the receiver is `mixed` and the call dispatches on the runtime
    /// class id. A Reflection owner is one of the candidates there, so the flag is refused on the
    /// same terms — but ONLY when every class declaring the method is an owner. A program that
    /// also has its own `getAttributes` may well be calling that one, and refusing it would be a
    /// compile error on valid PHP; the body's own throw covers the Reflection case at runtime.
    ///
    /// Refusing everything that is not `0` costs nothing in fidelity: PHP 8.5 accepts exactly two
    /// values and raises `ValueError: Argument #2 ($flags) must be a valid attribute filter flag`
    /// for the rest, measured with `1`, `3` and `4`. So the only valid value elephc turns away is
    /// `IS_INSTANCEOF` itself.
    ///
    /// Three spellings are deliberately allowed through:
    /// - a literal `null` name, or no name at all, because PHP ignores `$flags` entirely when
    ///   nothing is filtered and an absent `$name` IS null — `getAttributes(flags: 2)` and
    ///   `getAttributes(null, 2)` are the same call, and both answer with every attribute;
    /// - `$flags` that folds to `0`, which is what the body implements;
    /// - a literal `false`, which PHP coerces to `0` for this `int` parameter (measured: it
    ///   answers as `0` does, so refusing it would be a compile error on a working program).
    pub(in crate::types::checker::inference::objects) fn reject_unsupported_reflection_attribute_filter_flags(
        &self,
        class_name: Option<&str>,
        method_key: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<(), CompileError> {
        if method_key != "getattributes" || args.is_empty() {
            return Ok(());
        }
        let Some(owner) = self.reflection_attribute_owner(class_name, method_key) else {
            return Ok(());
        };
        let Some(sig) = self
            .classes
            .get(owner)
            .and_then(|class_info| class_info.methods.get(method_key))
        else {
            return Ok(());
        };
        // Consume the shared plan so named arguments and static associative spreads use exactly
        // the same parameter mapping as ordinary calls. A dynamic spread remains Hidden because
        // it can supply either filter at run time.
        let (name, flags) = get_attributes_filter_arguments(sig, args, span);
        let flags = match flags {
            FilterFlags::Absent => return Ok(()),
            // A named `flags:` can stand alone, and then `$name` takes its `null` default.
            FilterFlags::Given(expr) => {
                if name.is_none() {
                    return Ok(());
                }
                Some(expr)
            }
            // A spread hides the name as well, so nothing here can conclude it is null.
            FilterFlags::Hidden => None,
            FilterFlags::Invalid => return Ok(()),
        };
        // PHP returns every attribute when `$name` is null, whatever `$flags` says, so a null
        // name needs no subclass test and the flag is inert.
        if name
            .as_ref()
            .is_some_and(|name| matches!(name.kind, ExprKind::Null))
        {
            return Ok(());
        }
        let folds_to_zero = |expr: &Expr| {
            matches!(expr.kind, ExprKind::BoolLiteral(false))
                || self.eval_static_int_expr(expr) == Some(0)
        };
        if flags.as_ref().is_some_and(folds_to_zero) {
            return Ok(());
        }
        let receiver = match class_name {
            Some(_) => format!("{}::getAttributes()", owner),
            None => "getAttributes()".to_string(),
        };
        let cause = if flags.is_some() {
            "the $flags argument is not supported yet"
        } else {
            "the $flags argument cannot be read through a spread, so it is not accepted; \
             pass the arguments positionally"
        };
        Err(CompileError::new(
            span,
            &format!(
                "{}: {} — ReflectionAttribute::IS_INSTANCEOF needs a subclass test on a class \
                 name known only at runtime, and AOT mode has no name-keyed class hierarchy query",
                receiver, cause
            ),
        ))
    }

    /// Returns the Reflection owner whose `getAttributes()` this call can reach, if any.
    ///
    /// A named receiver must BE one of the owners. An unknown (`mixed`) receiver dispatches on the
    /// runtime class id over every class that declares the method, so any registered owner counts.
    fn reflection_attribute_owner(
        &self,
        class_name: Option<&str>,
        method_key: &str,
    ) -> Option<&'static str> {
        let declares = |owner: &'static str| {
            self.classes
                .get(owner)
                .is_some_and(|class_info| class_info.methods.contains_key(method_key))
        };
        let Some(key) = class_name.map(php_symbol_key) else {
            // An unknown receiver: refuse only when no other class could be the target.
            let foreign = self.classes.iter().any(|(name, class_info)| {
                class_info.methods.contains_key(method_key)
                    && !REFLECTION_ATTRIBUTE_OWNERS
                        .iter()
                        .any(|owner| php_symbol_key(owner) == php_symbol_key(name))
            });
            if foreign {
                return None;
            }
            return REFLECTION_ATTRIBUTE_OWNERS
                .iter()
                .copied()
                .find(|owner| declares(owner));
        };
        REFLECTION_ATTRIBUTE_OWNERS
            .iter()
            .copied()
            .find(|owner| php_symbol_key(owner) == key && declares(owner))
    }
}
