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

/// Generic child redeclarations discard the parent's default and leave typed storage unset.
#[test]
fn execute_program_asymmetric_main_merge_generic_default_override() {
    let program = parse_fragment(br#"class Base<T> { public int $value = 7; }
class Child<T> extends Base<T> { public int $value; }
$child = new Child<int>(); echo isset($child->value) ? 'bad' : 'unset'; return true;"#)
        .expect("parse generic property redeclarations");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values)
        .expect("allocate a generic child with an uninitialized override");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert_eq!(values.output, "unset");
}

/// Legal readonly storage keeps the abstract setter from writing its backing slot.
#[test]
fn execute_program_asymmetric_eighth_review_abstract_setter_storage() {
    let program = parse_fragment(br#"abstract class Base { abstract public int $x { set; } }
abstract class Child extends Base {
    public public(set) readonly int $x;
    public function probe(): bool { $this->x = 4; return isset($this->x); }
}
class Grandchild extends Child {}
return (new Grandchild())->probe();"#).expect("parse retained abstract setter");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values)
        .expect("a concrete descendant can retain the abstract setter");
    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// An abstract child can store readonly values without implementing its inherited abstract setter.
#[test]
fn execute_program_asymmetric_latest_review_abstract_set_readonly() {
    let program = parse_fragment(br#"abstract class Base { abstract public int $x { get; set; } }
abstract class Child extends Base { public public(set) readonly int $x; }
return true;"#).expect("parse abstract readonly storage");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values)
        .expect("abstract parent does not impose concrete readonly invariance");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Rechecking an abstract ancestor's legal readonly storage does not reject a later subclass.
#[test]
fn execute_program_asymmetric_latest_review_abstract_readonly_storage_descendant() {
    let program = parse_fragment(br#"abstract class Base { abstract public int $x { get; set; } }
abstract class Child extends Base { public public(set) readonly int $x; }
class Grandchild extends Child {}
return true;"#).expect("parse abstract readonly ancestor");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values)
        .expect("legal ancestor storage retains its declaration context");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Broadening the abstract exception never permits a concrete missing setter or narrowed access.
#[test]
fn execute_program_asymmetric_latest_review_abstract_set_readonly_refusals() {
    for child in [
        "class Child extends Base { public public(set) readonly int $x; }",
        "class Child extends Base { public readonly int $x; }",
        "abstract class Child extends Base { public readonly int $x; }",
    ] {
        let source = format!("abstract class Base {{ abstract public int $x {{ get; set; }} }} {child}");
        let program = parse_fragment(source.as_bytes()).expect("parse readonly refusals");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert_eq!(execute_program(&program, &mut scope, &mut values)
            .expect_err("setter visibility and concrete implementation remain required"), EvalStatus::RuntimeFatal);
    }
}

/// Eval rejects adding readonly to an inherited mutable property regardless of setter access.
#[test]
fn execute_program_asymmetric_fifth_review_rejects_added_readonly() {
    for (parent, child) in [
        ("public int $x;", "public readonly int $x;"),
        ("protected int $x;", "protected readonly int $x;"),
        ("public int $x;", "public public(set) readonly int $x;"),
        ("public int $x;", "public function __construct(public public(set) readonly int $x) {}"),
        ("public int $x { get => 1; }", "public readonly int $x;"),
        ("public int $x { get => 1; set {} }", "public public(set) readonly int $x;"),
    ] {
        let source = format!("class Base {{ {parent} }} class Child extends Base {{ {child} }} return 1;");
        let program = parse_fragment(source.as_bytes()).expect("parse readonly override");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert_eq!(execute_program(&program, &mut scope, &mut values)
            .expect_err("readonly cannot change across an override"), EvalStatus::RuntimeFatal);
    }
}

/// Only abstract getter-only contracts permit a readonly backed implementation.
#[test]
fn execute_program_asymmetric_ci_abstract_getter_readonly() {
    let program = parse_fragment(br#"abstract class Base { abstract public int $value { get; } }
class Child extends Base {
    public readonly int $value;
    public function __construct(int $value) { $this->value = $value; }
}
class Promoted extends Base { public function __construct(public readonly int $value) {} }
echo (new Child(42))->value, ":", (new Promoted(7))->value;
return true;"#).expect("parse readonly implementations");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values)
        .expect("readonly storage implements an abstract getter");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert_eq!(values.output, "42:7");
}

/// A private child cannot narrow a public or protected parent property, matching PHP.
#[test]
fn execute_program_asymmetric_oct9_rejects_private_child_narrowing() {
    for visibility in ["public", "protected"] {
        let source = format!("class Base {{ {visibility} int $value = 1; public function read() {{ return $this->value; }} }} class Child extends Base {{ private int $value = 2; }} return (new Child())->read();");
        let program = parse_fragment(source.as_bytes()).expect("parse property declarations");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert_eq!(execute_program(&program, &mut scope, &mut values)
            .expect_err("a private child reduces inherited read access"), EvalStatus::RuntimeFatal);
    }
}

/// Legal parent and child private slots keep their own defaults and declaring scope.
#[test]
fn execute_program_asymmetric_oct9_private_slots_keep_defaults() {
    let program = parse_fragment(br#"class Base { private int $value = 1; public function read() { return $this->value; } }
class Child extends Base { private int $value = 2; public function childRead() { return $this->value; } }
$child = new Child(); echo $child->read(), ':', $child->childRead(); return true;"#)
        .expect("parse separate private properties");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("separate private slots");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert_eq!(values.output, "1:2");
}

/// Public readonly properties without explicit set access accept protected-set redeclarations.
#[test]
fn execute_program_asymmetric_review_readonly_effective_set_visibility() {
    for source in [
        &b"class Base { public readonly int $x; } class Child extends Base { public protected(set) readonly int $x; } return 1;"[..],
        &b"readonly class Base { public int $x; } readonly class Child extends Base { public protected(set) int $x; } return 1;"[..],
        &b"class Base { public int $x { get => 1; } } class Child extends Base { public protected(set) int $x { get => 2; set { } } } return 1;"[..],
    ] {
        let program = parse_fragment(source).expect("parse property redeclaration");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let result = execute_program(&program, &mut scope, &mut values).expect("compatible set access");
        assert_eq!(values.get(result), FakeValue::Int(1));
    }
}

/// Omitting set access on a readonly child cannot reduce an explicitly public parent setter.
#[test]
fn execute_program_asymmetric_review_rejects_implicit_set_access_reduction() {
    let program = parse_fragment(b"class Base { public public(set) readonly int $x; } class Child extends Base { public readonly int $x; }")
        .expect("parse readonly properties");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert_eq!(execute_program(&program, &mut scope, &mut values).expect_err("narrowed setter"), EvalStatus::RuntimeFatal);
}

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
class EvalPropertyReadonlyPreserveBase {
    public public(set) readonly int $count;
}
class EvalPropertyReadonlyPreserveChild extends EvalPropertyReadonlyPreserveBase {
    public public(set) readonly int $count;
    public function __construct() { $this->count = 7; }
}
class EvalPropertyReadonlyWidenBase {
    protected readonly int $count;
    public function count() { return $this->count; }
}
class EvalPropertyReadonlyWidenChild extends EvalPropertyReadonlyWidenBase {
    public readonly int $count;
    public function __construct() { $this->count = 9; }
}
$box = new EvalPropertyRedeclareChild();
$box->value = "ok";
$readonly = new EvalPropertyReadonlyPreserveChild();
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
