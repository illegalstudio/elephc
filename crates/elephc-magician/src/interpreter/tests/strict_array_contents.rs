//! Purpose:
//! Exercises array-content constraints without pinning a binding's whole value kind.
//!
//! Called from:
//! - Focused inferred-array source contract and metadata regressions.
//!
//! Key details:
//! - Existing generic validators own candidate checks and keep failed stores unchanged.
//! - Conditional metadata is separate from ordinary typed-local declarations.

use super::super::*;
use super::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Installs a generated conditional schema before executing the fixture's source writes.
fn run_contents(source: &str, schema: &str, features: &[StrictFeature]) -> (String, FakeValue) {
    let schema = crate::parser::generics::parse_type_string(schema).unwrap();
    let contract = crate::eval_ir::array_contents_type(schema).unwrap();
    let mut context = ElephcEvalContext::new();
    let profile = features.iter().fold(SemanticProfile::PERMISSIVE,
        |profile, feature| profile.with(*feature, true));
    context.install_semantic_profile(profile).unwrap();
    let mut scope = ElephcEvalScope::new();
    scope.infer_local_type("items".to_string(), crate::eval_ir::parameter_type(&contract));
    let mut values = FakeOps::default();
    let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), profile).unwrap();
    let result = execute_statements(program.statements(), &mut context, &mut scope, &mut values)
        .unwrap_or_else(|error| panic!("{source}: {error:?}; {}", values.output));
    let EvalControl::Return(result) = result else { panic!("fixture returns its observed value"); };
    let returned = values.get(result); values.release(result).unwrap();
    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
    for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
    assert!(context.take_pending_throw().is_none());
    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: owner{id}"); }
    (values.output, returned)
}

/// Non-array replacements stay legal while array candidates retain exact element contracts.
#[test]
fn strict_array_contents_stores_preserve_independent_value_kinds() {
    let source = "$items=[1];try{$items[]='bad';}catch(TypeError $error){echo 'E';} \
        $items=7;echo ':',gettype($items);$items=null;echo ':',gettype($items); \
        $items=[2];try{$items=['bad'];}catch(TypeError $error){echo ':R';}return $items[0];";
    for features in [vec![StrictFeature::ArrayTypes], vec![StrictFeature::ArrayTypes, StrictFeature::Nullability]] {
        let (output, returned) = run_contents(source, "array<int>", &features);
        assert_eq!(output, "E:integer:NULL:R"); assert_eq!(returned, FakeValue::Int(2));
    }
}

/// Semantic integer keys allow hash gaps while the independent exact-key policy retains strings.
#[test]
fn strict_array_contents_keys_preserve_layout_and_normalization_policy() {
    for (features, expected) in [(vec![StrictFeature::ArrayTypes], "K"),
        (vec![StrictFeature::ArrayTypes, StrictFeature::ArrayKeys], "NK")] {
        let source = "$items=[2=>1];try{$items['3']=2;}catch(TypeError $error){echo 'N';} \
            try{$items['other']=3;}catch(TypeError $error){echo 'K';}return $items[2];";
        let (output, returned) = run_contents(source, "array<int>", &features);
        assert_eq!(output, expected); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// Explicit mixed/union elements remain open and aliases cannot bypass a closed source schema.
#[test]
fn strict_array_contents_aliases_and_explicit_element_alternatives() {
    for schema in ["array<int|string>", "array<mixed>"] {
        let (output, returned) = run_contents("$items=[1];$items[]='allowed';return $items[1];",
            schema, &[StrictFeature::ArrayTypes]);
        assert!(output.is_empty()); assert_eq!(returned, FakeValue::String("allowed".to_string()));
    }
    let source = "$items=[1];$alias=&$items;try{$alias[]='bad';}catch(TypeError $error){echo 'A';} \
        $alias=7;$alias=[2];return $items[0];";
    let (output, returned) = run_contents(source, "array<int>", &[StrictFeature::ArrayTypes]);
    assert_eq!(output, "A"); assert_eq!(returned, FakeValue::Int(2));
}
