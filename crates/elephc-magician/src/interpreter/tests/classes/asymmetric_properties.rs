//! Purpose:
//! Interpreter tests for asymmetric property visibility and interface,
//! abstract-class, inheritance, and readonly compatibility rules.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Read and write visibility are exercised from owner, hierarchy, and external
//!   scopes.

use super::super::super::*;
use super::super::support::*;

/// Verifies eval-declared asymmetric properties allow owner and subclass writes as PHP does.
#[test]
fn execute_program_allows_asymmetric_property_writes_from_allowed_scopes() {
    let program = parse_fragment(
        br#"class EvalAsymWriteBase {
    public private(set) int $privateValue = 1;
    public protected(set) string $protectedName = "base";
    public function ownerWrite($value, $name) {
        $this->privateValue = $value;
        $this->protectedName = $name;
    }
}
class EvalAsymWriteChild extends EvalAsymWriteBase {
    public function childWrite($name) {
        $this->protectedName = $name;
    }
}
$box = new EvalAsymWriteChild();
echo $box->privateValue; echo ":"; echo $box->protectedName; echo ":";
$box->ownerWrite(7, "owner");
echo $box->privateValue; echo ":"; echo $box->protectedName; echo ":";
$box->childWrite("child");
echo $box->protectedName;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:base:7:owner:child");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval-declared `private(set)` throws Error without dispatching `__set`.
#[test]
fn execute_program_private_set_property_write_outside_declaring_class_throws_error() {
    let program = parse_fragment(
        br#"class EvalAsymPrivateSetBox {
    public private(set) int $value = 1;
    public function __set($name, $value) {
        echo "bad";
    }
}
$box = new EvalAsymPrivateSetBox();
try {
    $box->value = 2;
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
        "Error:Cannot modify private(set) property EvalAsymPrivateSetBox::$value from global scope"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval-declared `protected(set)` throws Error for global writes.
#[test]
fn execute_program_protected_set_property_write_outside_hierarchy_throws_error() {
    let program = parse_fragment(
        br#"class EvalAsymProtectedSetBox {
    public protected(set) int $value = 1;
}
$box = new EvalAsymProtectedSetBox();
try {
    $box->value = 2;
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
        "Error:Cannot modify protected(set) property EvalAsymProtectedSetBox::$value from global scope"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies asymmetric write restrictions cannot satisfy a public interface set contract.
#[test]
fn execute_program_rejects_private_set_property_for_interface_set_contract() {
    let program = parse_fragment(
        br#"interface EvalAsymSetContract {
    public int $value { get; set; }
}
class EvalAsymSetContractBox implements EvalAsymSetContract {
    public private(set) int $value = 1;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("private(set) property should fail public interface set contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies asymmetric write restrictions cannot satisfy a public abstract set contract.
#[test]
fn execute_program_rejects_private_set_property_for_abstract_set_contract() {
    let program = parse_fragment(
        br#"abstract class EvalAsymAbstractSetBase {
    abstract public int $value { get; set; }
}
class EvalAsymAbstractSetBox extends EvalAsymAbstractSetBase {
    public private(set) int $value = 1;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("private(set) property should fail public abstract set contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval interface protected(set) property contracts accept compatible implementations.
#[test]
fn execute_program_allows_interface_protected_set_property_contract() {
    let program = parse_fragment(
        br#"interface EvalAsymProtectedSetContract {
    public protected(set) string $name { get; set; }
}
class EvalAsymProtectedSetBase implements EvalAsymProtectedSetContract {
    public protected(set) string $name = "base";
}
class EvalAsymProtectedSetChild extends EvalAsymProtectedSetBase {
    public function rename($name) { $this->name = $name; }
}
$box = new EvalAsymProtectedSetChild();
echo $box->name; echo ":";
$box->rename("child");
echo $box->name;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "base:child");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies private(set) interface contracts are final and cannot be implemented by a class.
#[test]
fn execute_program_rejects_private_set_interface_property_contract_implementation() {
    let program = parse_fragment(
        br#"interface EvalAsymPrivateSetInterfaceContract {
    public private(set) int $value { get; set; }
}
class EvalAsymPrivateSetInterfaceBox implements EvalAsymPrivateSetInterfaceContract {
    public private(set) int $value = 1;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("private(set) interface contract should be final");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies private(set) abstract properties behave as final contracts.
#[test]
fn execute_program_rejects_private_set_abstract_property_redeclaration() {
    let program = parse_fragment(
        br#"abstract class EvalAsymPrivateSetAbstractBase {
    abstract public private(set) int $value { get; set; }
}
class EvalAsymPrivateSetAbstractBox extends EvalAsymPrivateSetAbstractBase {
    public private(set) int $value = 1;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("private(set) abstract property should be final");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval property redeclarations may widen visibility while preserving invariant types.
#[test]
fn execute_program_accepts_compatible_property_redeclarations() {
    let program = parse_fragment(
        br#"class EvalPropertyRedeclareBase {
    protected int|string $value;
}
class EvalPropertyRedeclareChild extends EvalPropertyRedeclareBase {
    public string|int $value;
}
class EvalPropertyRelativeBase {
    public self $selfValue;
    public EvalPropertyRelativeBase $parentValue;
}
class EvalPropertyRelativeChild extends EvalPropertyRelativeBase {
    public self $selfValue;
    public parent $parentValue;
}
class EvalPropertyReadonlyAddBase {
    public int $count = 0;
}
class EvalPropertyReadonlyAddChild extends EvalPropertyReadonlyAddBase {
    public readonly int $count;
    public function __construct() { $this->count = 7; }
}
class EvalPropertyReadonlyWidenBase {
    protected int $count = 0;
    public function count() { return $this->count; }
}
class EvalPropertyReadonlyWidenChild extends EvalPropertyReadonlyWidenBase {
    public readonly int $count;
    public function __construct() { $this->count = 9; }
}
$box = new EvalPropertyRedeclareChild();
$box->value = "ok";
$readonly = new EvalPropertyReadonlyAddChild();
$widened = new EvalPropertyReadonlyWidenChild();
return $box->value . ":" . $readonly->count . ":" . $widened->count . ":" . $widened->count();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("ok:7:9:9".to_string()));
}

/// Verifies eval rejects inherited property redeclarations that violate PHP invariance.
#[test]
fn execute_program_rejects_incompatible_property_redeclarations() {
    let incompatible_type = parse_fragment(
        br#"class EvalPropertyTypeBase {
    public int $value;
}
class EvalPropertyStringChild extends EvalPropertyTypeBase {
    public string $value;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&incompatible_type, &mut scope, &mut values)
        .expect_err("incompatible inherited property type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let reduced_visibility = parse_fragment(
        br#"class EvalPropertyPublicBase {
    public int $value;
}
class EvalPropertyProtectedChild extends EvalPropertyPublicBase {
    protected int $value;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&reduced_visibility, &mut scope, &mut values)
        .expect_err("reduced inherited property visibility should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let typed_from_untyped = parse_fragment(
        br#"class EvalPropertyUntypedBase {
    public $value;
}
class EvalPropertyTypedChild extends EvalPropertyUntypedBase {
    public int $value;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&typed_from_untyped, &mut scope, &mut values)
        .expect_err("typed inherited property redeclaration should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let static_mismatch = parse_fragment(
        br#"class EvalPropertyStaticBase {
    public static int $value;
}
class EvalPropertyInstanceChild extends EvalPropertyStaticBase {
    public int $value;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&static_mismatch, &mut scope, &mut values)
        .expect_err("static inherited property redeclaration should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let readonly_mismatch = parse_fragment(
        br#"class EvalPropertyReadonlyBase {
    public readonly int $value;
}
class EvalPropertyMutableChild extends EvalPropertyReadonlyBase {
    public int $value;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&readonly_mismatch, &mut scope, &mut values)
        .expect_err("readonly inherited property redeclaration should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let reduced_write_visibility = parse_fragment(
        br#"class EvalPropertyProtectedSetBase {
    public protected(set) int $value;
}
class EvalPropertyPrivateSetChild extends EvalPropertyProtectedSetBase {
    public private(set) int $value;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&reduced_write_visibility, &mut scope, &mut values)
        .expect_err("reduced inherited property write visibility should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies readonly class inheritance requires matching readonly status.
#[test]
fn execute_program_rejects_readonly_class_extending_non_readonly_parent() {
    let program = parse_fragment(
        br#"class EvalReadonlyParentMismatchBase {}
readonly class EvalReadonlyParentMismatchChild extends EvalReadonlyParentMismatchBase {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly class cannot extend non-readonly parent");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
