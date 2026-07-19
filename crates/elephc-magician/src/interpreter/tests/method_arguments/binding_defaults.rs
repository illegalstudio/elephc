//! Purpose:
//! Interpreter tests for named, default, constant-default, and variadic argument
//! binding on eval methods and constructors.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover named arguments and named unpacking on instance methods,
//!   static methods, and constructors declared inside eval fragments.

use super::super::super::*;
use super::super::support::*;

/// Verifies eval-declared instance, static, and constructor methods bind named args.
#[test]
fn execute_program_binds_eval_method_named_args() {
    let program = parse_fragment(
        br#"class EvalNamedMethodBox {
    public function __construct($left, $right) {
        $this->label = $left . $right;
    }
    public function read($left, $right) {
        return $this->label . ":" . $left . ":" . $right;
    }
    public static function join($left, $right) {
        return $left . "-" . $right;
    }
}
$box = new EvalNamedMethodBox(right: "B", left: "A");
echo $box->read(right: "D", left: "C"); echo ":";
$args = ["right" => "F", "left" => "E"];
echo $box->read(...$args); echo ":";
return EvalNamedMethodBox::join(right: "H", left: "G");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "AB:C:D:AB:E:F:");
    assert_eq!(values.get(result), FakeValue::String("G-H".to_string()));
}

/// Verifies eval-declared methods use default values for omitted arguments.
#[test]
fn execute_program_binds_eval_method_default_args() {
    let program = parse_fragment(
        br#"class EvalDefaultMethodBox {
    public function __construct($left = "A", $right = "B") {
        $this->label = $left . $right;
    }
    public function read($left, $right = "D") {
        return $this->label . ":" . $left . ":" . $right;
    }
    public static function join($left = "G", $right = "H") {
        return $left . "-" . $right;
    }
}
$box = new EvalDefaultMethodBox();
echo $box->read("C"); echo ":";
echo $box->read(right: "F", left: "E"); echo ":";
return EvalDefaultMethodBox::join();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "AB:C:D:AB:E:F:");
    assert_eq!(values.get(result), FakeValue::String("G-H".to_string()));
}

/// Verifies eval-declared methods materialize constant-expression parameter defaults.
#[test]
fn execute_program_binds_eval_method_constant_default_args() {
    let program = parse_fragment(
        br#"define("EVAL_METHOD_DEFAULT_GLOBAL", "G");
class EvalDefaultConstBase {
    const LABEL = "base";
}
interface EvalDefaultConstIface {
    const WORD = "iface";
}
class EvalDefaultConstDep {
    public function __construct($label = "dep") {
        $this->label = $label;
    }
    public function read() {
        return $this->label;
    }
}
class EvalDefaultConstBox extends EvalDefaultConstBase {
    const LABEL = "box";
    public function __construct($label = self::LABEL) {
        $this->label = $label;
    }
    public function read($global = EVAL_METHOD_DEFAULT_GLOBAL, $parent = parent::LABEL, $iface = EvalDefaultConstIface::WORD, $class = self::class, $parentClass = parent::class, $items = [self::LABEL => 1 + 2, "fallback" => null ?? "fallback"], $method = __METHOD__, $dep = new EvalDefaultConstDep(label: "dep"), $clone = new self("inner")) {
        return $this->label . ":" . $global . ":" . $parent . ":" . $iface . ":" . $class . ":" . $parentClass . ":" . $items[self::LABEL] . ":" . $items["fallback"] . ":" . $method . ":" . $dep->read() . ":" . $clone->label;
    }
    public static function join($label = self::LABEL, $parent = parent::LABEL) {
        return $label . "-" . $parent;
    }
}
$box = new EvalDefaultConstBox();
echo $box->read(); echo ":";
return EvalDefaultConstBox::join();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "box:G:base:iface:EvalDefaultConstBox:EvalDefaultConstBase:3:fallback:EvalDefaultConstBox::read:dep:inner:"
    );
    assert_eq!(values.get(result), FakeValue::String("box-base".to_string()));
}

/// Verifies eval-declared methods bind positional and named values into variadic arrays.
#[test]
fn execute_program_binds_eval_method_variadic_args() {
    let program = parse_fragment(
        br#"class EvalVariadicMethodBox {
    public function __construct(...$parts) {
        $this->label = $parts[0] . $parts["right"];
    }
    public function read($head, ...$tail) {
        echo count($tail); echo ":";
        return $this->label . ":" . $head . ":" . $tail[0] . ":" . $tail["named"] . ":" . $tail["tail"];
    }
    public static function join(...$items) {
        return $items[0] . $items[1];
    }
}
$box = new EvalVariadicMethodBox("A", right: "B");
echo $box->read("C", "D", named: "E", tail: "F"); echo ":";
return EvalVariadicMethodBox::join("G", "H");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:AB:C:D:E:F:");
    assert_eq!(values.get(result), FakeValue::String("GH".to_string()));
}
