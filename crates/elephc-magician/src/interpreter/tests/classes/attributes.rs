//! Purpose:
//! Interpreter tests for class-related builtin attributes and readonly
//! inheritance constraints.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Attribute targets and `Override` declarations are validated at eval time.

use super::super::super::*;
use super::super::support::*;

/// Verifies eval validates PHP's global `#[Override]` method marker.
#[test]
fn execute_program_validates_override_attribute_targets() {
    let valid = parse_fragment(
        br#"interface EvalOverrideContract {
    public function label(): string;
}
class EvalOverrideBase {
    public function name(): string { return "base"; }
}
class EvalOverrideChild extends EvalOverrideBase implements EvalOverrideContract {
    #[\Override]
    public function name(): string { return "child"; }
    #[Override]
    public function label(): string { return "contract"; }
}
$box = new EvalOverrideChild();
echo $box->name() . ":" . $box->label();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&valid, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "child:contract");

    let invalid = parse_fragment(
        br#"class EvalOverrideMissing {
    #[\Override]
    public function missing(): string { return "bad"; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&invalid, &mut scope, &mut values)
        .expect_err("override marker without target should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies interface `#[Override]` methods require an inherited interface method.
#[test]
fn execute_program_validates_interface_override_attribute_targets() {
    let valid = parse_fragment(
        br#"interface EvalIfaceOverrideParent {
    public function label(): string;
}
interface EvalIfaceOverrideChild extends EvalIfaceOverrideParent {
    #[\Override]
    public function label(): string;
}
class EvalIfaceOverrideImpl implements EvalIfaceOverrideChild {
    public function label(): string { return "child"; }
}
$box = new EvalIfaceOverrideImpl();
echo $box->label();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&valid, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "child");

    let builtin_parent = parse_fragment(
        br#"interface EvalIfaceOverrideStringable extends Stringable {
    #[\Override]
    public function __toString(): string;
}
class EvalIfaceOverrideStringableImpl implements EvalIfaceOverrideStringable {
    public function __toString(): string { return "stringable"; }
}
$box = new EvalIfaceOverrideStringableImpl();
echo $box;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&builtin_parent, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "stringable");

    let invalid = parse_fragment(
        br#"interface EvalIfaceOverrideMissing {
    #[\Override]
    public function missing(): string;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&invalid, &mut scope, &mut values)
        .expect_err("interface override marker without parent method should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval rejects global builtin attributes on unsupported OOP targets.
#[test]
fn execute_program_rejects_invalid_builtin_attribute_targets() {
    let cases: &[(&[u8], &str)] = &[
        (
            br#"#[\AllowDynamicProperties] interface EvalInvalidAttrInterface {}"#,
            "AllowDynamicProperties interface",
        ),
        (
            br#"#[\AllowDynamicProperties] trait EvalInvalidAttrTrait {}"#,
            "AllowDynamicProperties trait",
        ),
        (
            br#"#[\AllowDynamicProperties] enum EvalInvalidAttrEnum { case Ready; }"#,
            "AllowDynamicProperties enum",
        ),
        (
            br#"#[\Override] class EvalInvalidAttrClass {}"#,
            "Override class",
        ),
        (
            br#"class EvalInvalidAttrProperty { #[\Override] public int $value; }"#,
            "Override property",
        ),
        (
            br#"class EvalInvalidAttrConstant { #[\AllowDynamicProperties] public const VALUE = 1; }"#,
            "AllowDynamicProperties constant",
        ),
        (
            br#"class EvalInvalidAttrMethod { #[\AllowDynamicProperties] public function run() {} }"#,
            "AllowDynamicProperties method",
        ),
        (
            br#"enum EvalInvalidAttrCase { #[\AllowDynamicProperties] case Ready; }"#,
            "AllowDynamicProperties enum case",
        ),
    ];

    for &(source, label) in cases {
        let program = parse_fragment(source).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let err = execute_program(&program, &mut scope, &mut values).expect_err(label);

        assert_eq!(err, EvalStatus::RuntimeFatal);
    }
}

/// Verifies readonly classes leave static properties mutable like ordinary classes.
#[test]
fn execute_program_allows_readonly_class_static_property() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyStaticBox {
    public static int $count = 1;
}
EvalReadonlyStaticBox::$count = EvalReadonlyStaticBox::$count + 1;
echo EvalReadonlyStaticBox::$count;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2");
    assert_eq!(values.get(result), FakeValue::Null);
}

/// Verifies readonly classes may extend readonly parents and use inherited constructors.
#[test]
fn execute_program_allows_readonly_class_extending_readonly_parent() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyParentBase {
    public int $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
readonly class EvalReadonlyParentChild extends EvalReadonlyParentBase {}
$box = new EvalReadonlyParentChild(13);
echo $box->id(); echo ":";
return $box->id();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "13:");
    assert_eq!(values.get(result), FakeValue::Int(13));
}
