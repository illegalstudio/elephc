//! Purpose:
//! Verifies PHP registration and fetch identity for eval global constants.
//!
//! Called from:
//! - The Magician interpreter unit-test module.
//!
//! Key details:
//! - Namespace case, literal separators, original inventory spelling and predefined
//!   names must remain coherent across define(), defined() and constant().

use super::super::*;
use super::support::*;

/// Namespace aliases share one definition while the constant basename preserves case.
#[test]
fn execute_program_constant_namespace_identity_matches_php() {
    let program = parse_fragment(br#"echo define("App\\VALUE", 4) ? "Y" : "N";
echo define("app\\VALUE", 5) ? "Y" : "N";
echo define("APP\\value", 6) ? "Y" : "N";
echo defined("aPP\\VALUE") ? "Y" : "N";
echo defined("app\\Value") ? "Y" : "N";
echo \App\VALUE, ":", \APP\value;
$all = get_defined_constants();
echo isset($all["App\\VALUE"]) ? "Y" : "N";
echo isset($all["app\\VALUE"]) ? "Y" : "N";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "YNYYN4:6YN");
    assert_eq!(values.warnings, vec![define_already_defined_warning(r"app\VALUE")]);
}

/// A definition keeps literal separators while constant() fetch removes exactly one.
#[test]
fn execute_program_constant_literal_root_separators_match_php() {
    let program = parse_fragment(br#"echo define("FOO", 4) ? "Y" : "N";
echo define("\\FOO", 5) ? "Y" : "N";
echo define("\\\\FOO", 6) ? "Y" : "N";
echo constant("FOO"), ":", constant("\\FOO"), ":", constant("\\\\FOO"), ":", constant("\\\\\\FOO"), "\n";
"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "YYY4:4:5:6\n");
    assert!(values.warnings.is_empty(), "{:?}", values.warnings);
}

/// A prefixed definition bypasses predefined-name lookup and retains its warning spelling.
#[test]
fn execute_program_constant_prefixed_predefined_name_is_distinct() {
    let program = parse_fragment(br#"echo define("\\NAN", 9) ? "Y" : "N";
echo define("\\NAN", 10) ? "Y" : "N";
echo constant("\\\\NAN"), ":", is_nan(NAN) ? "nan" : "other", "\n";
"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "YN9:nan\n");
    assert_eq!(values.warnings, vec![define_already_defined_warning(r"\NAN")]);
}

/// Native inventory snapshots expose the declaration spelling rather than the folded lookup key.
#[test]
fn constant_native_inventory_preserves_original_namespace_spelling() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let cell = values.int(7).expect("int cell");
    assert!(context.define_constant(r"App\VALUE", cell));
    assert_eq!(context.constant_inventory_entry(0).map(|(name, _)| name), Some(r"App\VALUE"));
    assert!(context.has_constant(r"app\VALUE"));
}
