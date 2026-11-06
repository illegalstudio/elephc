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

/// Relative retained setters restore caller scopes on both successful and rejected arguments.
#[test]
fn execute_program_readonly_setter_greptile_current_scope_restoration() {
    for hint in ["self", "parent"] {
        let storage = if hint == "self" { "Base" } else { "Root" };
        let source = format!("class Root {{}} abstract class Base extends Root {{ abstract public {hint} $x {{ set; }} }} abstract class Child extends Base {{ public public(set) readonly {storage} $x; }} class Grandchild extends Child {{}} return new Grandchild();");
        let program = parse_fragment(source.as_bytes()).unwrap();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let object = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        context.push_class_scope("CallerScope");
        context.push_called_class_scope("LateScope");
        eval_inherited_abstract_setter_write("Child", "x", object, &mut context, &mut values).unwrap();
        let invalid = values.int(7).unwrap();
        assert_eq!(eval_inherited_abstract_setter_write("Child", "x", invalid, &mut context, &mut values),
            Err(EvalStatus::UncaughtThrowable));
        let throwable = context.take_pending_throw().unwrap();
        eval_release_value(&mut context, &mut values, throwable).unwrap();
        eval_release_value(&mut context, &mut values, invalid).unwrap();
        assert_eq!(context.current_class_scope(), Some("CallerScope"));
        assert_eq!(context.current_called_class_scope(), Some("LateScope"));
        context.pop_called_class_scope(); context.pop_class_scope();
        eval_release_value(&mut context, &mut values, object).unwrap();
    }
}

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
