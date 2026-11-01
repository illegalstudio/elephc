//! Purpose:
//! Rejects a `ReflectionX::getAttributes()` call that passes a literal `null` `$flags`.
//!
//! Called from:
//! - `crate::types::checker::inference::objects::methods` before instance method inference, for
//!   both a known receiver class and a `mixed` receiver dispatched on the runtime class id.
//!
//! Key details:
//! - The argument list is read in every spelling PHP allows (named, static associative spread,
//!   literal spread), through the SHARED planner, so the refusal cannot be dodged by respelling
//!   the certain case. A spread of a runtime array is left to the body.
//! - Every other flag reaches the synthesized body, which checks it at run time: PHP's
//!   `ValueError` for a value other than `0` and `2`, the subclass/interface filter for
//!   `IS_INSTANCEOF`, and the plain name filter otherwise.

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
    /// Rejects a `getAttributes()` call whose `$flags` is the literal `null`.
    ///
    /// PHP coerces a null `$flags` to `0` behind a deprecation notice, as it does for any
    /// internal `int` parameter; the synthesized method receives the null itself and would
    /// reject it as an invalid flag, so the literal is refused rather than miscompiled. Every
    /// other flag is left to the body, which checks it at run time like PHP.
    ///
    /// `class_name` is `None` when the receiver is `mixed` and the call dispatches on the runtime
    /// class id. The literal is refused there only when every class declaring the method is a
    /// Reflection owner, since a program's own `getAttributes` may be the one being called.
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
        // the same parameter mapping as ordinary calls.
        let (_, flags) = get_attributes_filter_arguments(sig, args, span);
        // An absent flag is PHP's `0`, a runtime spread is checked by the body, and an invalid
        // plan is ordinary call validation's diagnostic.
        let FilterFlags::Given(flags) = flags else {
            return Ok(());
        };
        let receiver = match class_name {
            Some(_) => format!("{}::getAttributes()", owner),
            None => "getAttributes()".to_string(),
        };
        if matches!(flags.kind, ExprKind::Null) {
            return Err(CompileError::new(
                span,
                &format!(
                    "{}: passing null to the $flags argument is not supported: PHP coerces it to 0 \
                     with a deprecation notice; pass 0 or omit the argument",
                    receiver
                ),
            ));
        }
        Ok(())
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
