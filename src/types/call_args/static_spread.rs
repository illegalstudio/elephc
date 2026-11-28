//! Purpose:
//! Expands static spread-array arguments into positional / named argument forms.
//! Recognizes PHP array-unpack entries that can be resolved at compile time before planning.
//!
//! Called from:
//! - `crate::types::call_args::planner`
//!
//! Key details:
//! - The selected key policy keeps exact strings named and integer keys positional.
//! - Duplicate keys retain the existing static expansion and runtime fallback rules.

mod bindings;

use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;

/// Tracks whether a spread element came from the original source, a statically-known
/// associative positional element, a statically-known named element, or a positional
/// element that was part of a static tail after `...unpack_named`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExpandedArgOrigin {
    Source,
    StaticAssocPositional,
    StaticAssocNamed,
    StaticTailPositional,
}

/// The result of expanding static spread arrays from the call site.
///
/// `args` contains the flattened list of positional/named expressions;
/// `origins` parallels `args` and marks where each element came from.
pub(crate) struct ExpandedStaticSpreadArgs {
    pub(crate) args: Vec<Expr>,
    pub(crate) origins: Vec<ExpandedArgOrigin>,
}

/// Returns `true` if `args` contains any `NamedArg` expressions or spread
/// expressions that expand to a static associative array with named keys.
pub(crate) fn has_named_args(args: &[Expr], profile: elephc_semantics::SemanticProfile) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::NamedArg { .. } => true,
        ExprKind::Spread(inner) => static_assoc_spread_has_named_args(inner, profile),
        _ => false,
    })
}

/// Expands each `...$array` spread in `args` that refers to a static associative
/// array literal into separate `NamedArg` or positional expressions; returns the
/// flattened list. Indexed and non-static spreads are left as-is so runtime
/// arity guards and named-unpack tail planning still see their source nodes.
pub(crate) fn expand_static_assoc_spread_args(args: &[Expr], profile: elephc_semantics::SemanticProfile) -> Vec<Expr> {
    expand_static_assoc_spread_args_with_origins(args, profile).args
}

/// Like `expand_static_assoc_spread_args` but also returns an `origins` vector
/// parallel to the expanded arguments, marking where each element originated.
pub(crate) fn expand_static_assoc_spread_args_with_origins(
    args: &[Expr],
    profile: elephc_semantics::SemanticProfile,
) -> ExpandedStaticSpreadArgs {
    let mut expanded_args = Vec::with_capacity(args.len());
    let mut origins = Vec::with_capacity(args.len());

    for arg in args {
        if let ExprKind::Spread(inner) = &arg.kind {
            if let Some(mut spread_args) = expand_static_spread(inner, arg.span, profile) {
                for (arg, origin) in spread_args.drain(..) {
                    expanded_args.push(arg);
                    origins.push(origin);
                }
                continue;
            }
        }
        expanded_args.push(arg.clone());
        origins.push(ExpandedArgOrigin::Source);
    }

    ExpandedStaticSpreadArgs {
        args: expanded_args,
        origins,
    }
}

/// Rejects a statically known unpack that supplies a positional entry after a named one.
///
/// php walks each unpacked array in order and throws "Cannot use positional argument after named
/// argument during unpacking" at the first integer key that follows a string key IN THAT ARRAY
/// (`...["a" => "x", 1 => 2]`). A later, separate unpack may still supply positional values:
/// `f(...["a" => 10], ...[20])` binds `20` to the next parameter. The expansion below hoists
/// positional entries ahead of named ones, so this has to run on the source order first; without
/// it the call bound `$a` by name and `2` to the next free slot. Only literal keys are judged.
pub(crate) fn validate_static_unpack_order(
    args: &[Expr],
    profile: elephc_semantics::SemanticProfile,
) -> Result<(), super::plan::CallArgPlanError> {
    for arg in args {
        let ExprKind::Spread(inner) = &arg.kind else {
            continue;
        };
        let ExprKind::ArrayLiteralAssoc(pairs) = &inner.kind else {
            continue;
        };
        if bindings::plan(pairs, profile).is_some_and(|plan| plan.has_positional_after_named()) {
            return Err(super::plan::CallArgPlanError::PositionalAfterNamedUnpack { span: arg.span });
        }
    }
    Ok(())
}
/// Projects a planned positional call into values for builtin signature checking.
/// Only fully static indexed unpacks are flattened; callers must validate source
/// ordering first. General planning retains spread nodes for runtime arity guards.
pub(crate) fn expand_planned_positional_spreads(args: &[Expr], profile: elephc_semantics::SemanticProfile) -> Option<Vec<Expr>> {
    if has_named_args(args, profile) || !args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
        return None;
    }
    let mut expanded = Vec::new();
    for arg in args {
        if let ExprKind::Spread(inner) = &arg.kind {
            let ExprKind::ArrayLiteral(items) = &inner.kind else {
                return None;
            };
            if items.iter().any(|item| matches!(item.kind, ExprKind::Spread(_))) {
                return None;
            }
            expanded.extend(items.iter().cloned());
        } else {
            expanded.push(arg.clone());
        }
    }
    Some(expanded)
}

/// Combines a planned by-value indexed-unpack tail without changing named-key semantics.
/// Callers supply storage knowledge and must validate source ordering before projection.
/// Literal elements are flattened, but dynamic sources remain evaluated exactly once.
pub(crate) fn coalesce_planned_indexed_spreads(
    args: &[Expr],
    is_indexed: impl FnMut(&Expr) -> bool,
    profile: elephc_semantics::SemanticProfile,
) -> Option<Vec<Expr>> {
    coalesce_planned_indexed_spreads_with_prefix(args, usize::MAX, is_indexed, profile)
}

/// Includes surplus fixed arguments in a by-value variadic unpack without changing source order.
pub(crate) fn coalesce_planned_indexed_spreads_with_prefix(
    args: &[Expr],
    regular_param_count: usize,
    mut is_indexed: impl FnMut(&Expr) -> bool,
    profile: elephc_semantics::SemanticProfile,
) -> Option<Vec<Expr>> {
    if has_named_args(args, profile) {
        return None;
    }
    let first = args.iter().position(|arg| matches!(arg.kind, ExprKind::Spread(_)))?;
    let prefix = first.min(regular_param_count);
    if args.len() - first < 2 && prefix == first {
        return None;
    }
    let mut items = args[prefix..first].to_vec();
    for arg in &args[first..] {
        let ExprKind::Spread(inner) = &arg.kind else { return None };
        if !is_indexed(inner) {
            return None;
        }
        if let ExprKind::ArrayLiteral(elements) = &inner.kind {
            items.extend(elements.iter().cloned());
        } else {
            items.push(arg.clone());
        }
    }
    let span = args[first].span;
    let tail = if items.len() == 1 && matches!(items[0].kind, ExprKind::Spread(_)) {
        items.pop().unwrap()
    } else {
        Expr::new(ExprKind::Spread(Box::new(
            Expr::new(ExprKind::ArrayLiteral(items), span),
        )), span)
    };
    let mut projected = args[..prefix].to_vec();
    projected.push(tail);
    Some(projected)
}

/// Returns `true` if `expr` is an `ArrayLiteralAssoc` and any of its keys
/// resolve to a named (string) key in PHP array-unpack semantics.
///
/// A literal that mixes keys with a nested spread (`ExprKind::ArrayLiteralMixed`) contributes
/// its nested source's keys, which are unknowable here. Answering "yes, this may carry named
/// arguments" is the safe direction: every caller treats `true` as "decline the static shortcut
/// and keep the call dynamic".
fn static_assoc_spread_has_named_args(expr: &Expr, profile: elephc_semantics::SemanticProfile) -> bool {
    match &expr.kind {
        ExprKind::ArrayLiteralMixed(_) => true,
        ExprKind::ArrayLiteralAssoc(pairs) => pairs.iter().any(|(key, _)| {
            matches!(static_assoc_spread_key(key, profile), Some(StaticAssocSpreadKey::Named(_)))
        }),
        _ => false,
    }
}

/// If `expr` is a static associative array literal, returns its key/value pairs
/// as expanded `NamedArg` and positional expressions. Otherwise returns `None`.
fn expand_static_spread(
    expr: &Expr,
    spread_span: Span,
    profile: elephc_semantics::SemanticProfile,
) -> Option<Vec<(Expr, ExpandedArgOrigin)>> {
    match &expr.kind {
        ExprKind::ArrayLiteralAssoc(pairs) => expand_static_assoc_spread(pairs, spread_span, profile),
        _ => None,
    }
}

/// Converts an associative array literal's pairs into a flat list of expressions.
/// String keys become `NamedArg` expressions with `ExpandedArgOrigin::StaticAssocNamed`;
/// integer-like keys become positional expressions with `ExpandedArgOrigin::StaticAssocPositional`.
/// Duplicate typed keys keep the whole spread opaque so array construction can evaluate
/// every overwritten value before the runtime binder observes the collapsed array.
fn expand_static_assoc_spread(
    pairs: &[(Expr, Expr)],
    spread_span: Span,
    profile: elephc_semantics::SemanticProfile,
) -> Option<Vec<(Expr, ExpandedArgOrigin)>> {
    let plan = bindings::plan(pairs, profile)?;
    plan.can_expand().then(|| plan.into_arguments(spread_span))
}

/// Projects only final value bindings for inference; execution retains the original source array.
pub(crate) fn static_assoc_spread_bindings_for_inference(
    source: &Expr, profile: elephc_semantics::SemanticProfile,
) -> Option<Vec<Expr>> {
    let ExprKind::ArrayLiteralAssoc(pairs) = &source.kind else { return None; };
    Some(bindings::plan(pairs, profile)?.into_arguments(source.span)
        .into_iter().map(|(value, _)| value).collect())
}

/// Classifies a single array literal key as either a positional (integer-like)
/// key or a named (string) key according to PHP array-unpack semantics.
#[derive(Clone, PartialEq, Eq, Hash)]
enum StaticAssocSpreadKey {
    Positional(i64),
    Named(String),
}

/// Returns `Some(StaticAssocSpreadKey::Positional)` for integer-like literal keys,
/// `Some(StaticAssocSpreadKey::Named(name))` for non-integer string literals,
/// or `None` if the key is not a statically resolvable literal.
fn static_assoc_spread_key(key: &Expr, profile: elephc_semantics::SemanticProfile) -> Option<StaticAssocSpreadKey> {
    match &key.kind {
        ExprKind::IntLiteral(value) => Some(StaticAssocSpreadKey::Positional(*value)),
        ExprKind::BoolLiteral(value) if !profile.contains(elephc_semantics::StrictFeature::ArrayKeys) => {
            Some(StaticAssocSpreadKey::Positional(i64::from(*value)))
        }
        ExprKind::FloatLiteral(value) if !profile.contains(elephc_semantics::StrictFeature::ArrayKeys) => {
            Some(StaticAssocSpreadKey::Positional(crate::types::php_float_array_key(*value)))
        }
        ExprKind::StringLiteral(value) => {
            let bytes = elephc_builtin_contract::string_literal::literal_bytes(value);
            match elephc_semantics::array_keys::classify_string_key(profile, &bytes) {
                elephc_semantics::array_keys::StringKey::Integer(value) => Some(StaticAssocSpreadKey::Positional(value)),
                elephc_semantics::array_keys::StringKey::String(_) =>
                    String::from_utf8(bytes).ok().map(StaticAssocSpreadKey::Named),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an indexed literal unpack without hiding its source spread node.
    fn indexed_spread(items: Vec<Expr>) -> Expr {
        Expr::new(
            ExprKind::Spread(Box::new(Expr::new(
                ExprKind::ArrayLiteral(items),
                Span::dummy(),
            ))),
            Span::dummy(),
        )
    }

    /// Variadic projection keeps the fixed prefix and evaluates surplus sources before the unpack.
    #[test]
    fn coalesced_indexed_spreads_include_variadic_positionals() {
        let tail = Expr::new(ExprKind::Spread(Box::new(Expr::var("rest"))), Span::dummy());
        let args = vec![Expr::var("x"), Expr::var("y"), tail.clone()];
        let projected = coalesce_planned_indexed_spreads_with_prefix(&args, 1, |_| true, elephc_semantics::SemanticProfile::PERMISSIVE).unwrap();
        assert_eq!(projected[0], args[0]);
        assert_eq!(projected[1], indexed_spread(vec![args[1].clone(), tail]));
        assert_eq!(args.len(), 3);
        assert!(coalesce_planned_indexed_spreads_with_prefix(&args, 1, |_| false, elephc_semantics::SemanticProfile::PERMISSIVE).is_none());
        assert!(coalesce_planned_indexed_spreads_with_prefix(&args, 2, |_| true, elephc_semantics::SemanticProfile::PERMISSIVE).is_none());
    }

    /// Literal unpacks remain visible to the runtime required-argument guard.
    #[test]
    fn indexed_spread_preserves_runtime_arity_shape() {
        let arg = indexed_spread(vec![Expr::int_lit(10)]);
        let expanded = expand_static_assoc_spread_args_with_origins(&[arg.clone()], elephc_semantics::SemanticProfile::PERMISSIVE);
        assert_eq!(expanded.args, vec![arg]);
        assert!(expanded.origins == vec![ExpandedArgOrigin::Source]);
    }

    /// Empty static unpacks disappear without evaluating or cloning a dynamic source twice.
    #[test]
    fn coalesced_indexed_spreads_preserve_dynamic_source() {
        let dynamic = Expr::new(ExprKind::Spread(Box::new(Expr::var("names"))), Span::dummy());
        let args = [indexed_spread(Vec::new()), dynamic.clone(), indexed_spread(Vec::new())];
        assert_eq!(coalesce_planned_indexed_spreads(&args, |_| true, elephc_semantics::SemanticProfile::PERMISSIVE), Some(vec![dynamic]));
        assert_eq!(args.len(), 3, "the source planner input is unchanged");
    }

    /// Named-key sources and positional-after-unpack shapes are not merged as indexed arrays.
    #[test]
    fn coalesced_indexed_spreads_reject_nonindexed_or_invalid_tails() {
        let dynamic = Expr::new(ExprKind::Spread(Box::new(Expr::var("names"))), Span::dummy());
        assert!(coalesce_planned_indexed_spreads(
            &[indexed_spread(Vec::new()), dynamic.clone()], |_| false,
            elephc_semantics::SemanticProfile::PERMISSIVE,
        ).is_none());
        assert!(coalesce_planned_indexed_spreads(&[dynamic, Expr::int_lit(1)], |_| true, elephc_semantics::SemanticProfile::PERMISSIVE).is_none());
    }

    /// Builtin signature projection counts unpacked values instead of containers.
    #[test]
    fn planned_positional_spreads_count_values_not_containers() {
        let expanded = expand_planned_positional_spreads(&[
            indexed_spread(Vec::new()),
            indexed_spread(vec![Expr::int_lit(10)]),
            indexed_spread(vec![Expr::int_lit(20)]),
        ], elephc_semantics::SemanticProfile::PERMISSIVE).expect("all indexed unpacks are static");
        assert_eq!(
            expanded,
            vec![Expr::int_lit(10), Expr::int_lit(20)],
        );
    }

    /// A dynamic unpack leaves the entire argument list for runtime materialization.
    #[test]
    fn planned_positional_spreads_do_not_partially_expand_dynamic_calls() {
        let dynamic = Expr::new(
            ExprKind::Spread(Box::new(Expr::var("remaining"))),
            Span::dummy(),
        );
        assert!(expand_planned_positional_spreads(&[
            indexed_spread(vec![Expr::int_lit(10)]),
            dynamic,
        ], elephc_semantics::SemanticProfile::PERMISSIVE).is_none());
    }

    /// A positional unpack after named unpacking must reach the planner's tail path.
    #[test]
    fn indexed_spread_after_named_unpack_preserves_tail_origin() {
        let named = Expr::new(
            ExprKind::Spread(Box::new(Expr::new(
                ExprKind::ArrayLiteralAssoc(vec![(Expr::string_lit("a"), Expr::int_lit(10))]),
                Span::dummy(),
            ))),
            Span::dummy(),
        );
        let tail = indexed_spread(vec![Expr::int_lit(20)]);
        let expanded = expand_static_assoc_spread_args_with_origins(&[named, tail.clone()], elephc_semantics::SemanticProfile::PERMISSIVE);
        assert!(matches!(expanded.args[0].kind, ExprKind::NamedArg { .. }));
        assert_eq!(expanded.args[1], tail);
        assert!(expanded.origins == vec![
            ExpandedArgOrigin::StaticAssocNamed,
            ExpandedArgOrigin::Source,
        ]);
        assert!(expand_planned_positional_spreads(&expanded.args, elephc_semantics::SemanticProfile::PERMISSIVE).is_none());
    }

    /// An unpacked literal that itself contains a spread stays one dynamic argument.
    ///
    /// The parser files such a literal as `ExprKind::ArrayLiteralMixed`, which has no static
    /// expansion: the nested source decides which keys exist, so the whole unpack keeps its
    /// original node and its single evaluation.
    #[test]
    fn nested_assoc_spread_keeps_the_whole_unpack_dynamic() {
        let arg = Expr::new(
            ExprKind::Spread(Box::new(Expr::new(
                ExprKind::ArrayLiteralMixed(vec![
                    crate::parser::ast::ArrayEntry::Keyed(Expr::string_lit("a"), Expr::int_lit(10)),
                    crate::parser::ast::ArrayEntry::Spread(Expr::new(
                        ExprKind::Spread(Box::new(Expr::var("extra"))),
                        Span::dummy(),
                    )),
                ]),
                Span::dummy(),
            ))),
            Span::dummy(),
        );
        let expanded = expand_static_assoc_spread_args_with_origins(&[arg.clone()], elephc_semantics::SemanticProfile::PERMISSIVE);
        assert_eq!(expanded.args, vec![arg.clone()]);
        assert!(expanded.origins == vec![ExpandedArgOrigin::Source]);
        // The nested source can supply named keys, so the static named-argument shortcut declines.
        assert!(has_named_args(&[arg.clone()], elephc_semantics::SemanticProfile::PERMISSIVE));
        assert!(expand_planned_positional_spreads(&[arg.clone()], elephc_semantics::SemanticProfile::PERMISSIVE).is_none());
        assert!(coalesce_planned_indexed_spreads(&[arg.clone(), arg], |_| true, elephc_semantics::SemanticProfile::PERMISSIVE).is_none());
    }

    /// Duplicate PHP-normalized positional keys retain the original spread for runtime collapse.
    #[test]
    fn duplicate_normalized_positional_keys_keep_source_array_evaluation() {
        let arg = Expr::new(
            ExprKind::Spread(Box::new(Expr::new(
                ExprKind::ArrayLiteralAssoc(vec![
                    (Expr::int_lit(0), Expr::int_lit(1)),
                    (
                        Expr::new(ExprKind::BoolLiteral(false), Span::dummy()),
                        Expr::int_lit(2),
                    ),
                    (Expr::string_lit("0"), Expr::int_lit(3)),
                ]),
                Span::dummy(),
            ))),
            Span::dummy(),
        );
        let expanded = expand_static_assoc_spread_args_with_origins(&[arg.clone()], elephc_semantics::SemanticProfile::PERMISSIVE);
        assert_eq!(expanded.args, vec![arg]);
        assert!(expanded.origins == vec![ExpandedArgOrigin::Source]);
    }

    /// Numeric string names remain distinct from integers under the independently selected policy.
    #[test]
    fn exact_static_unpack_preserves_numeric_names_and_source_bytes() {
        use elephc_semantics::{SemanticProfile, StrictFeature};
        let dynamic = "<?php $object->{$method}(...['name' => 1, '0' => 2]);";
        assert!(crate::parser::parse(&crate::lexer::tokenize(dynamic).unwrap()).is_ok());
        let unpack = |pairs| Expr::new(ExprKind::Spread(Box::new(Expr::new(
            ExprKind::ArrayLiteralAssoc(pairs), Span::dummy(),
        ))), Span::dummy());
        for exact in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            for name in ["0", "1", "-1", "01", "a\0b"] {
                let input = unpack(vec![(Expr::string_lit(name), Expr::int_lit(7))]);
                let expanded = expand_static_assoc_spread_args(&[input], profile);
                let named = exact || matches!(name, "01" | "a\0b");
                assert_eq!(has_named_args(&expanded, profile), named, "{name:?} exact={exact}");
                if named {
                    assert!(matches!(&expanded[0].kind, ExprKind::NamedArg { name: actual, value }
                        if actual == name && value.kind == ExprKind::IntLiteral(7)));
                } else { assert_eq!(expanded[0].kind, ExprKind::IntLiteral(7)); }
            }
            let input = unpack(vec![(Expr::int_lit(0), Expr::int_lit(10)),
                (Expr::string_lit("0"), Expr::int_lit(20))]);
            let expanded = expand_static_assoc_spread_args(&[input.clone()], profile);
            if exact { assert_eq!(expanded.len(), 2); }
            else { assert_eq!(expanded, vec![input]); }
            let input = unpack(vec![(Expr::string_lit("name"), Expr::int_lit(1)),
                (Expr::string_lit("0"), Expr::int_lit(2))]);
            assert!(validate_syntax_only(&[input.clone()]).is_ok());
            assert_eq!(validate_static_unpack_order(&[input], profile).is_err(), !exact);
            let input = unpack(vec![(Expr::new(ExprKind::BoolLiteral(false), Span::dummy()), Expr::int_lit(3))]);
            let expanded = expand_static_assoc_spread_args(&[input.clone()], profile);
            if exact { assert_eq!(expanded, vec![input]); }
            else { assert_eq!(expanded[0].kind, ExprKind::IntLiteral(3)); }
        }
    }

    /// Checks the same parser-only ordering used before semantic profiles are selected.
    fn validate_syntax_only(args: &[Expr]) -> Result<(), super::super::plan::CallArgPlanError> {
        super::super::planner::validate_syntactic_call_argument_order(args)
    }


    /// Final bindings can guide inference while execution still evaluates every overwritten value.
    #[test]
    fn duplicate_unpack_projection_keeps_execution_opaque_and_inference_precise() {
        use elephc_semantics::{SemanticProfile, StrictFeature};
        let source = Expr::new(ExprKind::ArrayLiteralAssoc(vec![
            (Expr::string_lit("value"), Expr::var("first")),
            (Expr::string_lit("value"), Expr::var("last")),
        ]), Span::dummy());
        let unpack = Expr::new(ExprKind::Spread(Box::new(source.clone())), Span::dummy());
        for exact in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            assert_eq!(expand_static_assoc_spread_args(&[unpack.clone()], profile), vec![unpack.clone()]);
            let values = static_assoc_spread_bindings_for_inference(&source, profile).unwrap();
            assert!(matches!(&values[0].kind, ExprKind::NamedArg { name, value }
                if name == "value" && value.kind == ExprKind::Variable("last".into())));
        }
    }

    /// Typed overwrites retain their first iteration position before unpack order is checked.
    #[test]
    fn duplicate_unpack_order_uses_final_typed_keys_and_float_identity() {
        use elephc_semantics::{SemanticProfile, StrictFeature};
        let unpack = |pairs| Expr::new(ExprKind::Spread(Box::new(Expr::new(
            ExprKind::ArrayLiteralAssoc(pairs), Span::dummy(),
        ))), Span::dummy());
        let source = unpack(vec![(Expr::string_lit("0"), Expr::int_lit(1)),
            (Expr::string_lit("named"), Expr::int_lit(2)), (Expr::int_lit(0), Expr::int_lit(3))]);
        for exact in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            assert_eq!(validate_static_unpack_order(&[source.clone()], profile).is_err(), exact);
        }
        let source = unpack(vec![(Expr::int_lit(i64::MIN), Expr::int_lit(1)),
            (Expr::string_lit("named"), Expr::int_lit(2)),
            (Expr::new(ExprKind::FloatLiteral(-(i64::MIN as f64)), Span::dummy()), Expr::int_lit(3))]);
        assert!(validate_static_unpack_order(&[source.clone()], SemanticProfile::PERMISSIVE).is_ok());
        assert_eq!(expand_static_assoc_spread_args(&[source.clone()], SemanticProfile::PERMISSIVE), vec![source]);
    }

}
