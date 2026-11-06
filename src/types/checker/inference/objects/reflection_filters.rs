//! Purpose:
//! Rejects a `ReflectionX::getAttributes()` call that provably requests the unsupported
//! `ReflectionAttribute::IS_INSTANCEOF` subclass filter.
//!
//! Called from:
//! - `crate::types::checker::inference::objects::methods` before instance method inference, for
//!   both a known receiver class and a `mixed` receiver dispatched on the runtime class id.
//!
//! Key details:
//! - Only a `$flags` that folds to `IS_INSTANCEOF` next to a `$name` that is a string by
//!   construction is refused. Every other flag reaches the synthesized body, which checks it at
//!   run time: PHP's `ValueError` for a value other than `0` and `2`, a `ReflectionException` for
//!   `IS_INSTANCEOF` with a non-null name, and the name filter otherwise.
//! - The argument list is read in every spelling PHP allows (named, static associative spread,
//!   literal spread), through the SHARED planner, so the refusal cannot be dodged by respelling
//!   the certain case. A spread of a runtime array is left to the body.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{BinOp, Expr, ExprKind};
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

/// `ReflectionAttribute::IS_INSTANCEOF`, the only `$flags` value besides `0` PHP accepts.
const IS_INSTANCEOF: i64 = 2;

/// Returns whether a `$name` argument is a string by construction (a literal, a `::class`
/// fetch, or a concatenation), so a flag on it certainly requests the subclass filter.
fn name_is_known_string(name: &Expr) -> bool {
    matches!(
        name.kind,
        ExprKind::StringLiteral(_)
            | ExprKind::ClassConstant { .. }
            | ExprKind::ObjectClassName { .. }
            | ExprKind::BinaryOp {
                op: BinOp::Concat,
                ..
            }
    )
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
    /// Rejects a `getAttributes()` call that provably requests `ReflectionAttribute::IS_INSTANCEOF`
    /// for a class name.
    ///
    /// That flag widens the `$name` filter to subclasses and implemented interfaces. Deciding it
    /// needs a subclass test on the ATTRIBUTE's own class name, which the synthesized body only
    /// has as a runtime string, and every name-keyed hierarchy query refuses one in AOT mode:
    /// `is_subclass_of()` answers `false` for a string first operand (`static_relation_holds`
    /// requires `PhpType::Object`), and `class_parents()`, `class_implements()` and
    /// `class_exists()` reject a non-literal name outright (#1113). Honouring it by exact name
    /// would return a SUBSET of what PHP returns, with no diagnostic, so a call that certainly
    /// asks for it stays a compile error.
    ///
    /// Everything this cannot prove is left to the body, which checks the flag at run time and
    /// is loud about it: PHP 8.5 accepts exactly `0` and `2` and raises `ValueError: Argument #2
    /// ($flags) must be a valid attribute filter flag` for the rest (measured with `1`, `3`, `4`
    /// and `-1`, whatever `$name` holds), and `2` with a non-null name throws a
    /// `ReflectionException`. A runtime `0` (a variable, a spread of a runtime array) therefore
    /// just filters, and a runtime-null name ignores the flag as PHP does. A literal `null` flag
    /// is refused: PHP coerces it to `0` with a deprecation notice, which the call does not model.
    ///
    /// `class_name` is `None` when the receiver is `mixed` and the call dispatches on the runtime
    /// class id. A Reflection owner is one of the candidates there, so the flag is refused on the
    /// same terms — but ONLY when every class declaring the method is an owner. A program that
    /// also has its own `getAttributes` may well be calling that one, and refusing it would be a
    /// compile error on valid PHP; the body's own checks cover the Reflection case at runtime.
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
        let (name, flags) = get_attributes_filter_arguments(sig, args, span);
        // An absent flag is PHP's `0`, a runtime spread is checked by the body, and an invalid
        // plan is ordinary call validation's diagnostic.
        let FilterFlags::Given(flags) = flags else {
            return Ok(());
        };
        let receiver = match class_name {
            Some(_) => format!("{}::getAttributes()", owner),
            None => "getAttributes()".to_string(),
        };
        // PHP coerces a null `$flags` to `0` behind a deprecation notice, as it does for any
        // internal `int` parameter; the synthesized method receives the null itself and would
        // reject it as an invalid flag, so the literal is refused rather than miscompiled.
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
        if self.eval_static_int_expr(&flags) != Some(IS_INSTANCEOF) {
            return Ok(());
        }
        // A named `flags:` alone leaves `$name` at its `null` default, and PHP ignores the flag
        // when nothing is filtered; so does a name that is only null at run time.
        if !name.as_ref().is_some_and(name_is_known_string) {
            return Ok(());
        }
        Err(CompileError::new(
            span,
            &format!(
                "{}: the $flags argument is not supported yet: ReflectionAttribute::IS_INSTANCEOF \
                 needs a subclass test on a class name known only at runtime, and AOT mode has no \
                 name-keyed class hierarchy query",
                receiver
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
