//! Purpose:
//! Checks reference-source projections against authoritative call plans.
//!
//! Called from:
//! - Focused argument planner unit regressions.
//!
//! Key details:
//! - Shared dummy spans cannot merge source positions, and unpacked values are not caller places.

use super::*;
use crate::{parser::ast::{Expr, ExprKind}, span::Span};
use elephc_semantics::SemanticProfile;

/// Obtains real declared signatures without recreating their parameter/default metadata.
fn signatures() -> crate::types::CheckResult {
    let source = "<?php function take(mixed $read, mixed &$write, mixed &...$tail):void {} function optionalRef(mixed &$target=null):void {} function rest(mixed &...$tail):void {} function valueRest(mixed $read,mixed &$write,mixed ...$tail):void {} $a=null; $b=null; take($a,$b); optionalRef(); rest(); valueRest($a,$b);";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    crate::types::check_with_target(&program, crate::codegen::platform::Target::detect_host()).unwrap()
}

/// Builds a named caller argument without assigning it an artificial unique source position.
fn named(name: &str, value: Expr) -> Expr {
    Expr::new(ExprKind::NamedArg { name: name.into(), value: Box::new(value) }, Span::dummy())
}

/// Identical spans and names remain distinct source positions through positional and named plans.
#[test]
fn reference_sources_use_indices_for_shared_source_positions() {
    let checked = signatures(); let signature = &checked.functions["take"];
    let slot = Expr::var("slot");
    for (args, expected) in [
        (vec![slot.clone(), slot.clone(), slot.clone()], vec![1, 2]),
        (vec![named("write", slot.clone()), named("read", slot.clone()), named("extra", slot.clone())], vec![0, 2]),
        (vec![named("read", slot.clone()), named("write", slot.clone())], vec![1]),
    ] {
        let plan = plan_call_args(signature, &args, Span::dummy(), false, true, SemanticProfile::PERMISSIVE).unwrap();
        assert_eq!(plan.reference_source_indices(signature), expected);
    }
}

/// Values expanded from an unpack and declaration defaults never create caller reference sources.
#[test]
fn reference_sources_exclude_unpacked_values_and_defaults() {
    let checked = signatures(); let signature = &checked.functions["take"];
    let slot = Expr::var("slot");
    let unpack = Expr::new(ExprKind::Spread(Box::new(Expr::new(ExprKind::ArrayLiteralAssoc(vec![
        (Expr::string_lit("read"), slot.clone()), (Expr::string_lit("write"), slot.clone()),
    ]), Span::dummy()))), Span::dummy());
    let args = [unpack, named("extra", slot)];
    let plan = plan_call_args(signature, &args, Span::dummy(), false, true, SemanticProfile::PERMISSIVE).unwrap();
    assert_eq!(plan.reference_source_indices(signature), vec![2]);
    let optional = &checked.functions["optionalRef"];
    let plan = plan_call_args(optional, &[], Span::dummy(), false, true, SemanticProfile::PERMISSIVE).unwrap();
    assert!(plan.reference_source_indices(optional).is_empty());
}

/// Runtime unpacks and hidden implementation parameters cannot become plain caller reference sources.
#[test]
fn reference_sources_respect_dynamic_unpack_and_hidden_parameters() {
    let checked = signatures(); let signature = &checked.functions["rest"];
    let args = [Expr::new(ExprKind::Spread(Box::new(Expr::var("items"))), Span::dummy()),
        named("extra", Expr::var("slot"))];
    let plan = plan_call_args(signature, &args, Span::dummy(), false, true, SemanticProfile::PERMISSIVE).unwrap();
    assert_eq!(plan.reference_source_indices(signature), vec![1]);
    let mut hidden = checked.functions["valueRest"].clone();
    hidden.params.insert(2, ("capture".into(), crate::types::PhpType::Mixed));
    hidden.ref_params.insert(2, true);
    hidden.defaults.insert(2, None);
    let args = [Expr::var("slot"), Expr::var("slot"), named("capture", Expr::var("slot"))];
    let plan = super::planner::plan_call_args_with_regular_param_count(&hidden, &args,
        Span::dummy(), 2, false, true, SemanticProfile::PERMISSIVE).unwrap();
    assert_eq!(plan.reference_source_indices(&hidden), vec![1]);
}
