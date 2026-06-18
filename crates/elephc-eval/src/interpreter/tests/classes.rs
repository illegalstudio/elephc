//! Purpose:
//! Interpreter tests for eval-declared class runtime behavior.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases cover class property semantics that need eval runtime state.

use super::super::*;
use super::support::*;

/// Verifies readonly eval properties can be initialized inside their constructor.
#[test]
fn execute_program_initializes_readonly_property_in_constructor() {
    let program = parse_fragment(
        br#"class EvalReadonlyBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
$box = new EvalReadonlyBox(7);
echo $box->id(); echo ":";
return $box->id();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "7:");
    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies readonly eval properties reject writes outside the declaring constructor.
#[test]
fn execute_program_rejects_readonly_property_write_after_constructor() {
    let program = parse_fragment(
        br#"class EvalReadonlyBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyBox(7);
$box->replace(8);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly property write should fail outside constructor");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies readonly classes make instance properties readonly implicitly.
#[test]
fn execute_program_initializes_readonly_class_property_in_constructor() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyClassBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
$box = new EvalReadonlyClassBox(11);
echo $box->id(); echo ":";
return $box->id();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "11:");
    assert_eq!(values.get(result), FakeValue::Int(11));
}

/// Verifies readonly class instance properties reject writes after construction.
#[test]
fn execute_program_rejects_readonly_class_property_write_after_constructor() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyClassFailBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyClassFailBox(11);
$box->replace(12);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly class property write should fail outside constructor");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies readonly class static properties remain mutable.
#[test]
fn execute_program_allows_readonly_class_static_property_mutation() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyStaticBox {
    public static int $count = 1;
}
EvalReadonlyStaticBox::$count = 5;
echo EvalReadonlyStaticBox::$count; echo ":";
EvalReadonlyStaticBox::$count = EvalReadonlyStaticBox::$count + 1;
return EvalReadonlyStaticBox::$count;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:");
    assert_eq!(values.get(result), FakeValue::Int(6));
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
