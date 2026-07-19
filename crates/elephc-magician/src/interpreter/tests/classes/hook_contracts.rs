//! Purpose:
//! Interpreter tests for inherited, interface, abstract, and trait property-hook
//! contracts.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Tests distinguish get-only, set-required, readonly, type, and final/abstract
//!   compatibility failures.

use super::super::super::*;
use super::super::support::*;

/// Verifies get-only property hooks throw Error on writes outside a set accessor.
#[test]
fn execute_program_write_to_get_only_eval_property_hook_throws_error() {
    let program = parse_fragment(
        br#"class EvalHookReadOnly {
    public int $answer {
        get => 42;
    }
}
$box = new EvalHookReadOnly();
try {
    $box->answer = 7;
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Property EvalHookReadOnly::$answer is read-only"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval subclasses inherit parent property hooks.
#[test]
fn execute_program_inherits_eval_property_hooks() {
    let program = parse_fragment(
        br#"class EvalHookBase {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
class EvalHookChild extends EvalHookBase {
    public function shout() { return $this->value . "?"; }
}
$box = new EvalHookChild();
$box->value = "Ada";
echo $box->value; echo ":";
return $box->shout();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada!:");
    assert_eq!(values.get(result), FakeValue::String("Ada!?".to_string()));
}

/// Verifies eval interface property hook contracts are enforced through inheritance.
#[test]
fn execute_program_accepts_interface_property_hook_contracts() {
    let program = parse_fragment(
        br#"interface EvalHookContract {
    public string $value { get; set; }
}
interface EvalNamedHookContract extends EvalHookContract {
    public string $name { get; }
}
class EvalHookContractBox implements EvalNamedHookContract {
    public string $name = "box";
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
$box = new EvalHookContractBox();
$box->value = "Ada";
echo $box->name; echo ":";
echo $box->value;
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "box:Ada!");
    assert_eq!(values.get(result), FakeValue::String("Ada!".to_string()));
}

/// Verifies a normal public mutable property satisfies an eval interface get/set contract.
#[test]
fn execute_program_accepts_plain_property_for_interface_hook_contracts() {
    let program = parse_fragment(
        br#"interface EvalPlainHookContract {
    public string $value { get; set; }
}
class EvalPlainHookContractBox implements EvalPlainHookContract {
    public string $value = "Ada";
}
$box = new EvalPlainHookContractBox();
echo $box->value; echo ":";
$box->value = "Grace";
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada:");
    assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
}

/// Verifies interface property hook types are checked on abstract and concrete classes.
#[test]
fn execute_program_validates_interface_property_hook_types() {
    let valid_program = parse_fragment(
        br#"interface EvalIfaceGetWide {
    public int|string $value { get; }
}
interface EvalIfaceSetNarrow {
    public int $slot { set; }
}
abstract class EvalIfacePropertyDeferred implements EvalIfaceGetWide {}
abstract class EvalIfacePropertyGood implements EvalIfaceGetWide, EvalIfaceSetNarrow {
    abstract public int $value { get; }
    abstract public int|string $slot { set; }
}
class EvalIfacePropertyConcrete implements EvalIfaceGetWide {
    public int $value = 4;
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&valid_program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.get(result), FakeValue::Bool(true));

    let bad_abstract_get = parse_fragment(
        br#"interface EvalIfaceGetInt {
    public int $value { get; }
}
abstract class EvalIfaceGetWideBad implements EvalIfaceGetInt {
    abstract public int|string $value { get; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_abstract_get, &mut scope, &mut values)
        .expect_err("wider abstract get property type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let bad_abstract_set = parse_fragment(
        br#"interface EvalIfaceSetWide {
    public int|string $value { set; }
}
abstract class EvalIfaceSetNarrowBad implements EvalIfaceSetWide {
    abstract public int $value { set; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_abstract_set, &mut scope, &mut values)
        .expect_err("narrower abstract set property type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let bad_concrete_get = parse_fragment(
        br#"interface EvalIfaceConcreteGetInt {
    public int $value { get; }
}
class EvalIfaceConcreteGetWideBad implements EvalIfaceConcreteGetInt {
    public int|string $value = 4;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_concrete_get, &mut scope, &mut values)
        .expect_err("wider concrete get property type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let bad_inherited_property = parse_fragment(
        br#"interface EvalIfaceInheritedGet {
    public int $value { get; }
}
abstract class EvalIfaceInheritedPropertyBase {
    public string $value = "bad";
}
abstract class EvalIfaceInheritedPropertyChild extends EvalIfaceInheritedPropertyBase implements EvalIfaceInheritedGet {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_inherited_property, &mut scope, &mut values)
        .expect_err("inherited incompatible interface property should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies a get-only hook cannot satisfy a writable eval interface contract.
#[test]
fn execute_program_rejects_get_only_hook_for_interface_set_contract() {
    let program = parse_fragment(
        br#"interface EvalHookSetContract {
    public int $answer { get; set; }
}
class EvalHookGetOnlyContractBox implements EvalHookSetContract {
    public int $answer {
        get => 42;
    }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("get-only hook should fail writable interface contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies readonly properties cannot satisfy writable eval interface contracts.
#[test]
fn execute_program_rejects_readonly_property_for_interface_set_contract() {
    let program = parse_fragment(
        br#"interface EvalReadonlyHookContract {
    public int $id { get; set; }
}
class EvalReadonlyHookContractBox implements EvalReadonlyHookContract {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly property should fail writable interface contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies concrete eval subclasses satisfy abstract property hook contracts.
#[test]
fn execute_program_accepts_abstract_property_hook_contracts() {
    let program = parse_fragment(
        br#"abstract class EvalAbstractHookBase {
    abstract public string $value { get; set; }
}
class EvalAbstractHookBox extends EvalAbstractHookBase {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
$box = new EvalAbstractHookBox();
$box->value = "Ada";
echo $box->value;
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada!");
    assert_eq!(values.get(result), FakeValue::String("Ada!".to_string()));
}

/// Verifies normal mutable properties satisfy abstract get/set hook contracts.
#[test]
fn execute_program_accepts_plain_property_for_abstract_hook_contracts() {
    let program = parse_fragment(
        br#"abstract class EvalPlainAbstractHookBase {
    abstract public string $value { get; set; }
}
class EvalPlainAbstractHookBox extends EvalPlainAbstractHookBase {
    public string $value = "Ada";
}
$box = new EvalPlainAbstractHookBox();
echo $box->value; echo ":";
$box->value = "Grace";
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada:");
    assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
}

/// Verifies concrete eval subclasses must declare inherited abstract properties.
#[test]
fn execute_program_rejects_missing_abstract_property_hook_contract() {
    let program = parse_fragment(
        br#"abstract class EvalMissingAbstractHookBase {
    abstract public string $value { get; }
}
class EvalMissingAbstractHookBox extends EvalMissingAbstractHookBase {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("missing abstract property should fail concrete subclass");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies abstract final eval properties are rejected while parsing.
#[test]
fn parse_fragment_rejects_final_abstract_property_hook_contract() {
    let err = parse_fragment(
        br#"abstract class EvalFinalAbstractHookBase {
    abstract final public string $value { get; }
}"#,
    )
    .expect_err("final abstract property should fail");

    assert_eq!(err, EvalParseError::UnsupportedConstruct);
}

/// Verifies readonly properties cannot satisfy abstract writable hook contracts.
#[test]
fn execute_program_rejects_readonly_property_for_abstract_set_contract() {
    let program = parse_fragment(
        br#"abstract class EvalReadonlyAbstractHookBase {
    abstract public int $id { get; set; }
}
class EvalReadonlyAbstractHookBox extends EvalReadonlyAbstractHookBase {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly property should fail abstract writable contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies abstract trait property hook contracts are enforced after trait expansion.
#[test]
fn execute_program_enforces_trait_abstract_property_hook_contracts() {
    let program = parse_fragment(
        br#"trait EvalTraitNeedsName {
    abstract protected string $name { get; }
    public function label() { return $this->name; }
}
class EvalTraitNameBox {
    use EvalTraitNeedsName;
    protected string $name = "Ada";
}
$box = new EvalTraitNameBox();
echo $box->label();
return $box->label();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada");
    assert_eq!(values.get(result), FakeValue::String("Ada".to_string()));
}
