//! Purpose:
//! Interpreter tests for private/protected member access, shadowing, and missing
//! method failures.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Declaring-class scope and global-scope failures use separate cases.

use super::super::super::*;
use super::super::support::*;

/// Verifies eval methods can access private properties and methods declared in their class.
#[test]
fn execute_program_allows_private_eval_members_inside_declaring_class() {
    let program = parse_fragment(
        br#"class EvalPrivateBox {
    private int $secret = 4;
    private function bump($n) { return $this->secret + $n; }
    public function read($n) { return $this->bump($n); }
}
$box = new EvalPrivateBox();
return $box->read(3);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}
/// Verifies protected eval members are accessible across a class hierarchy.
#[test]
fn execute_program_allows_protected_eval_members_from_related_classes() {
    let program = parse_fragment(
        br#"class EvalProtectedBase {
    protected int $base = 5;
    protected function add($n) { return $this->base + $n; }
}
class EvalProtectedChild extends EvalProtectedBase {
    public function read($n) { return $this->add($n); }
}
$box = new EvalProtectedChild();
return $box->read(2);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies eval child properties shadow private parent properties with a separate storage slot.
#[test]
fn execute_program_shadows_private_eval_parent_property_with_separate_slot() {
    let program = parse_fragment(
        br#"class EvalPrivateShadowBase {
    private $value = 1;

    public function parentValue() {
        return $this->value;
    }
}
class EvalPrivateShadowChild extends EvalPrivateShadowBase {
    public $value = "child";

    public function childValue() {
        return $this->value;
    }
}
$box = new EvalPrivateShadowChild();
echo $box->parentValue(); echo ":";
echo $box->childValue(); echo ":";
echo $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:child:child");
}

/// Verifies eval later redeclarations update the visible slot while preserving a private grandparent slot.
#[test]
fn execute_program_keeps_eval_private_grandparent_slot_after_later_redeclaration() {
    let program = parse_fragment(
        br#"class EvalPrivateGrandBase {
    private $value = 1;

    public function grandValue() {
        return $this->value;
    }
}
class EvalPrivateGrandParent extends EvalPrivateGrandBase {
    public $value = 2;

    public function parentValue() {
        return $this->value;
    }
}
class EvalPrivateGrandChild extends EvalPrivateGrandParent {
    public $value = 3;
}
$box = new EvalPrivateGrandChild();
echo $box->grandValue(); echo ":";
echo $box->parentValue(); echo ":";
echo $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:3:3");
}

/// Verifies eval throws Error for private property access from global scope.
#[test]
fn execute_program_private_eval_member_access_from_global_scope_throws_error() {
    let program = parse_fragment(
        br#"class EvalPrivateGlobalBox {
    private int $secret = 4;
    private function read() { return $this->secret; }
}
$box = new EvalPrivateGlobalBox();
try {
    echo $box->secret;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Cannot access private property EvalPrivateGlobalBox::$secret"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval throws Error for calls to private methods from global scope.
#[test]
fn execute_program_private_eval_method_call_from_global_scope_throws_error() {
    let program = parse_fragment(
        br#"class EvalPrivateMethodBox {
    private function read() { return 4; }
}
$box = new EvalPrivateMethodBox();
try {
    echo $box->read();
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Call to private method EvalPrivateMethodBox::read() from global scope"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies missing eval-declared instance methods throw PHP-compatible Error values.
#[test]
fn execute_program_missing_eval_method_call_throws_error() {
    let program = parse_fragment(
        br#"class EvalMissingMethodBox {}
$box = new EvalMissingMethodBox();
try {
    echo $box->missing();
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Call to undefined method EvalMissingMethodBox::missing()"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
