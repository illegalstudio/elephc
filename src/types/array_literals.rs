//! Purpose:
//! Plans typed literal-array keys and normalizes mixed syntax without evaluating values.
//!
//! Called from:
//! - AST constant folding and codegen default/constant materializers.
//! - Source numeric evidence before branch pruning.
//!
//! Key details:
//! - Implicit keys follow the selected PHP profile; source expressions keep their order.
//! - Overflow and nonliteral keys remain unresolved so runtime checks and effects survive.

use crate::parser::ast::{ArrayEntry, Expr, ExprKind};

use elephc_semantics::{array_keys::StringKey, SemanticProfile};

/// One literal entry in evaluation order, including overwrites of an earlier key.
pub(crate) struct LiteralArrayEntryPlan<'a> {
    /// Semantic key identity; strings retain their original bytes and kind in exact mode.
    pub key: StringKey<'a>,
    /// Explicit source syntax is retained so folding cannot erase key conversion diagnostics.
    pub key_expr: Option<&'a Expr>,
    /// Original value expression, never cloned or skipped when its key is overwritten.
    pub value: &'a Expr,
}

/// Plans typed keys and append history without evaluating or deduplicating source entries.
pub(crate) fn plan_literal_array_entries(
    expr: &ExprKind, php_version: crate::php_version::PhpVersion, profile: SemanticProfile,
) -> Option<Vec<LiteralArrayEntryPlan<'_>>> {
    let mut max_int: Option<i64> = None;
    let entries = match expr {
        ExprKind::ArrayLiteral(values) => values.iter().map(|value| (None, value)).collect::<Vec<_>>(),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs.iter().map(|(key, value)| (Some(key), value)).collect(),
        ExprKind::ArrayLiteralMixed(entries) => {
            let mut planned = Vec::with_capacity(entries.len());
            for entry in entries {
                match entry {
                    ArrayEntry::Spread(_) => return None,
                    ArrayEntry::Keyed(key, value) => planned.push((Some(key), value)),
                    ArrayEntry::Value(value) => planned.push((None, value)),
                }
            }
            planned
        }
        _ => return None,
    };
    let mut result = Vec::with_capacity(entries.len());
    for (key_expr, value) in entries {
        let key = match key_expr {
            Some(key) => super::literal_array_key(key, profile)?,
            None => StringKey::Integer(match max_int {
                None => 0,
                Some(max) if max < 0 && php_version < crate::php_version::PhpVersion::Php83 => 0,
                Some(max) => max.checked_add(1)?,
            }),
        };
        if let StringKey::Integer(key) = key { max_int = Some(max_int.map_or(key, |max| max.max(key))); }
        result.push(LiteralArrayEntryPlan { key, key_expr, value });
    }
    Some(result)
}

/// Normalizes spread-free mixed syntax while retaining every explicit key/value evaluation.
pub(crate) fn fold_spreadless_mixed_array_literal(
    expr: &ExprKind, php_version: crate::php_version::PhpVersion,
) -> Option<ExprKind> {
    if !matches!(expr, ExprKind::ArrayLiteralMixed(_)) { return None; }
    let entries = plan_literal_array_entries(expr, php_version, SemanticProfile::PERMISSIVE)?;
    let pairs = entries.into_iter().map(|entry| {
        let key = match entry.key_expr {
            Some(key) => key.clone(),
            None => {
                let StringKey::Integer(key) = entry.key else { unreachable!("implicit literal keys are integers"); };
                Expr::new(ExprKind::IntLiteral(key), entry.value.span)
            }
        };
        (key, entry.value.clone())
    }).collect();
    Some(ExprKind::ArrayLiteralAssoc(pairs))
}

/// Converts a float key using PHP's signed modulo-2^64 integer conversion.
/// Non-finite values become zero; diagnostics remain the caller's responsibility.
pub(crate) fn php_float_array_key(value: f64) -> i64 {
    elephc_semantics::numeric::php_float_to_int(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;
    use crate::php_version::PhpVersion;
    use elephc_semantics::StrictFeature;

    /// Parses original source syntax so the planner must preserve key conversion expressions.
    fn literal(source: &str) -> ExprKind {
        let mut program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let StmtKind::Assign { value, .. } = program.remove(0).kind else { panic!("expected a literal assignment"); };
        value.kind
    }

    /// Exact strings, versioned negative append history and conversion syntax share one plan.
    #[test]
    fn strict_literal_array_plan_keeps_typed_keys_history_and_source_evaluation() {
        let php = SemanticProfile::PERMISSIVE;
        let exact = php.with(StrictFeature::ArrayKeys, true);
        let input = literal("<?php $items = ['1' => 10, 20, '01' => 30, 1 => 40];");
        for (profile, expected) in [
            (php, vec![StringKey::Integer(1), StringKey::Integer(2), StringKey::String(b"01"), StringKey::Integer(1)]),
            (exact, vec![StringKey::String(b"1"), StringKey::Integer(0), StringKey::String(b"01"), StringKey::Integer(1)]),
        ] {
            let entries = plan_literal_array_entries(&input, PhpVersion::Php85, profile).unwrap();
            assert_eq!(entries.iter().map(|entry| entry.key).collect::<Vec<_>>(), expected);
            assert_eq!(entries.len(), 4, "overwritten values must still be evaluated");
            assert!(entries[1].key_expr.is_none());
        }
        let negative = literal("<?php $items = [-2 => 1, 2];");
        for (version, last) in [(PhpVersion::Php82, 0), (PhpVersion::Php83, -1)] {
            assert_eq!(plan_literal_array_entries(&negative, version, php).unwrap()[1].key, StringKey::Integer(last));
        }
        let floating = literal("<?php $items = [1.5 => 1, 2];");
        let Some(ExprKind::ArrayLiteralAssoc(folded)) = fold_spreadless_mixed_array_literal(&floating, PhpVersion::Php85)
            else { panic!("expected a mixed literal normalization"); };
        assert!(matches!(folded[0].0.kind, ExprKind::FloatLiteral(1.5)), "key conversion warnings must survive folding");
        assert_eq!(folded[1].0.kind, ExprKind::IntLiteral(2));
        for input in ["<?php $items = [true => 1];", "<?php $items = [1.5 => 1];", "<?php $items = [null => 1];"] {
            assert!(plan_literal_array_entries(&literal(input), PhpVersion::Php85, exact).is_none());
        }
    }
}
