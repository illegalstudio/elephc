//! Purpose:
//! Eval declaration regressions for contravariant readonly backing storage over abstract setters.
//!
//! Called from:
//! - The Magician interpreter unit test harness.
//!
//! Key details:
//! - Setter-only abstract requirements permit broader readonly storage in abstract children.
//! - Getter contracts, concrete missing setters and ordinary mutable properties stay restricted.

use super::super::super::*;
use super::super::support::*;

/// Executes one declaration-only fixture through the real class validation path.
fn verify(parent_prelude: &str, required: &str, storage: &str) {
    let source = format!("{parent_prelude} abstract class Base {{ abstract public {required} $x {{ set; }} }} abstract class Child extends Base {{ public public(set) readonly {storage} $x; }} class Grandchild extends Child {{}} return true;");
    let program = parse_fragment(source.as_bytes()).expect("parse broader readonly storage");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values)
        .expect("readonly storage accepts every inherited setter value");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// An integer setter permits integer-or-string readonly storage.
#[test]
fn execute_program_asymmetric_current_review_setter_union() { verify("", "int", "int|string"); }

/// Nullable setters require null to remain accepted by their broader storage.
#[test]
fn execute_program_asymmetric_current_review_setter_nullable() { verify("", "?int", "int|string|null"); }

/// Storage may accept a parent class of every value admitted by the setter.
#[test]
fn execute_program_asymmetric_current_review_setter_class_supertype() {
    verify("class ValueBase {} class Value extends ValueBase {}", "Value", "ValueBase");
}

/// A parent's lexical self is not rebound to the pending child when comparing storage.
#[test]
fn execute_program_asymmetric_current_review_setter_lexical_self() { verify("", "self", "Base|int"); }

/// Identical `self` spellings still resolve against different declaration owners.
#[test]
fn execute_program_asymmetric_tenth_review_setter_self_owner_mismatch() {
    let source = "abstract class Base { abstract public self $x { set; } } abstract class Child extends Base { public public(set) readonly self $x; } return true;";
    let program = parse_fragment(source.as_bytes()).expect("parse owner-relative self storage");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert_eq!(
        execute_program(&program, &mut scope, &mut values)
            .expect_err("child self storage cannot accept every parent self setter value"),
        EvalStatus::RuntimeFatal,
    );
}

/// The setter-only abstract readonly exception does not relax other declaration contracts.
#[test]
fn execute_program_asymmetric_current_review_setter_refusals() {
    for (parent, child) in [
        ("abstract public int $x { get; set; }", "abstract class Child extends Base { public public(set) readonly int|string $x; }"),
        ("abstract public int $x { set; }", "class Child extends Base { public public(set) readonly int|string $x; }"),
        ("abstract public ?int $x { set; }", "abstract class Child extends Base { public public(set) readonly int|string $x; }"),
        ("public int $x;", "abstract class Child extends Base { public public(set) readonly int|string $x; }"),
    ] {
        let source = format!("abstract class Base {{ {parent} }} {child} return true;");
        let program = parse_fragment(source.as_bytes()).expect("parse incompatible storage");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert_eq!(execute_program(&program, &mut scope, &mut values)
            .expect_err("the declaration is not legal setter-only readonly storage"), EvalStatus::RuntimeFatal);
    }
}
