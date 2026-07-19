//! Purpose:
//! Interpreter tests for by-reference constructor-promoted property aliases.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Cases cover variables, array elements, object/static storage, nested paths,
//!   defaults, and the readonly incompatibility.

use super::super::super::*;
use super::super::support::*;

/// Verifies by-reference promoted properties stay aliased to caller variables.
#[test]
fn execute_program_aliases_by_reference_promoted_variable_properties() {
    let program = parse_fragment(
        br#"class EvalPromotedRefBox {
    public function __construct(public &$value) {}
}
$value = 1;
$box = new EvalPromotedRefBox($value);
$box->value = 5;
echo $value; echo ":";
$value = 7;
echo $box->value;
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:7");
    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies by-reference promoted properties can alias caller array elements.
#[test]
fn execute_program_aliases_by_reference_promoted_array_element_properties() {
    let program = parse_fragment(
        br#"class EvalPromotedArrayRefBox {
    public function __construct(public &$value) {}
}
$items = [1];
$box = new EvalPromotedArrayRefBox($items[0]);
$box->value = 5;
echo $items[0]; echo ":";
$items[0] = 7;
echo $box->value;
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:7");
    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies by-reference promoted properties can alias caller object properties.
#[test]
fn execute_program_aliases_by_reference_promoted_object_property_properties() {
    let program = parse_fragment(
        br#"class EvalPromotedObjectRefHolder {
    public $value = 1;
}
class EvalPromotedObjectRefBox {
    public function __construct(public &$value) {}
}
$holder = new EvalPromotedObjectRefHolder();
$box = new EvalPromotedObjectRefBox($holder->value);
$box->value = 5;
echo $holder->value; echo ":";
$holder->value = 7;
echo $box->value;
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:7");
    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies by-reference promoted properties can alias static and nested property targets.
#[test]
fn execute_program_aliases_by_reference_promoted_static_and_nested_properties() {
    let program = parse_fragment(
        br#"class EvalPromotedStaticRefHolder {
    public static $value = 1;
    public $items = [1];
    public static $staticItems = [1];
}
class EvalPromotedStaticRefBox {
    public function __construct(public &$value) {}
}
$box = new EvalPromotedStaticRefBox(EvalPromotedStaticRefHolder::$value);
$box->value = 5;
echo EvalPromotedStaticRefHolder::$value; echo ":";
EvalPromotedStaticRefHolder::$value = 7;
echo $box->value; echo ":";
$holder = new EvalPromotedStaticRefHolder();
$itemBox = new EvalPromotedStaticRefBox($holder->items[0]);
$itemBox->value = 11;
echo $holder->items[0]; echo ":";
$holder->items[0] = 13;
echo $itemBox->value; echo ":";
$staticItemBox = new EvalPromotedStaticRefBox(EvalPromotedStaticRefHolder::$staticItems[0]);
$staticItemBox->value = 17;
echo EvalPromotedStaticRefHolder::$staticItems[0]; echo ":";
EvalPromotedStaticRefHolder::$staticItems[0] = 19;
return $staticItemBox->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:7:11:13:17:");
    assert_eq!(values.get(result), FakeValue::Int(19));
}

/// Verifies by-reference promoted defaults use internal property alias storage.
#[test]
fn execute_program_aliases_by_reference_promoted_default_properties() {
    let program = parse_fragment(
        br#"class EvalPromotedDefaultRefBox {
    public function __construct(public &$value = null) {}
}
$box = new EvalPromotedDefaultRefBox();
$box->value = 5;
echo $box->value;
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5");
    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies readonly by-reference promotion fails when the constructor creates the alias.
#[test]
fn execute_program_rejects_readonly_by_reference_promoted_properties() {
    let program = parse_fragment(
        br#"class EvalPromotedReadonlyRefBox {
    public function __construct(public readonly int &$value) {}
}
$value = 1;
new EvalPromotedReadonlyRefBox($value);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly by-reference promoted property should fail at construction");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
