//! Purpose:
//! Interpreter tests for eval debug-output builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These tests cover `print_r()` capture mode, variadic `var_dump()`, and
//!   object property/reference rendering without growing the generic expression suite.

use super::super::*;
use super::support::*;

/// Verifies eval `print_r()` emits values, returns true, and captures output when requested.
#[test]
fn execute_program_dispatches_print_r_builtin() {
    let program = parse_fragment(
        br#"print_r("x"); echo ":";
print_r(value: false); echo ":";
print_r([1, 2]); echo ":";
$call = call_user_func("print_r", true);
$spread = call_user_func_array("print_r", ["value" => "z"]);
$captured = print_r(["k" => 1], true);
$captured_call = call_user_func_array("print_r", ["value" => ["q" => 2], "return" => true]);
echo ":" . ($call ? "call" : "bad") . ":" . ($spread ? "spread" : "bad") . ":";
echo $captured === "Array\n(\n    [k] => 1\n)\n" ? "captured" : "bad"; echo ":";
echo $captured_call === "Array\n(\n    [q] => 2\n)\n" ? "callcaptured" : "bad"; echo ":";
return function_exists("print_r");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "x::",
            "Array\n",
            "(\n",
            "    [0] => 1\n",
            "    [1] => 2\n",
            ")\n",
            ":1z:call:spread:captured:callcaptured:",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies captured `print_r()` output includes eval object properties.
#[test]
fn execute_program_print_r_captures_object_properties() {
    let program = parse_fragment(
        br#"class EvalPrintRProps {
    public $a = 1;
    protected $b = "p";
    private $c = false;
    public function __construct(&$ref) {
        $this->a =& $ref;
        $this->dyn = [2];
    }
}
$x = 9;
$o = new EvalPrintRProps($x);
$x = 10;
return print_r($o, true);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let FakeValue::String(output) = values.get(result) else {
        panic!("expected captured print_r object output");
    };
    assert_eq!(values.output, "");
    assert!(output.contains("EvalPrintRProps Object\n"));
    assert!(output.contains("    [a] => 10\n"));
    assert!(output.contains("    [b:protected] => p\n"));
    assert!(output.contains("    [c:EvalPrintRProps:private] => \n"));
    assert!(output.contains("    [dyn] => Array\n"));
}

/// Verifies eval `var_dump()` emits scalar and array diagnostics and returns null.
#[test]
fn execute_program_dispatches_var_dump_builtin() {
    let program = parse_fragment(
        br#"var_dump(42);
var_dump("hi");
var_dump(false);
var_dump(null);
var_dump([10, 20]);
var_dump(["x" => true]);
var_dump(1, "m", [2]);
$call = call_user_func("var_dump", 3.5, false);
$spread = call_user_func_array("var_dump", ["value" => "z", "values" => true]);
echo ($call === null ? "call-null" : "bad") . ":" . ($spread === null ? "spread-null" : "bad") . ":";
return function_exists("var_dump");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "int(42)\n",
            "string(2) \"hi\"\n",
            "bool(false)\n",
            "NULL\n",
            "array(2) {\n",
            "  [0]=>\n",
            "  int(10)\n",
            "  [1]=>\n",
            "  int(20)\n",
            "}\n",
            "array(1) {\n",
            "  [\"x\"]=>\n",
            "  bool(true)\n",
            "}\n",
            "int(1)\n",
            "string(1) \"m\"\n",
            "array(1) {\n",
            "  [0]=>\n",
            "  int(2)\n",
            "}\n",
            "float(3.5)\n",
            "bool(false)\n",
            "string(1) \"z\"\n",
            "bool(true)\n",
            "call-null:spread-null:",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `var_dump()` keeps eval-declared and runtime object class names.
#[test]
fn execute_program_var_dump_prints_object_class_names() {
    let program = parse_fragment(
        br#"class EvalDumpBox {}
var_dump(new EvalDumpBox());
var_dump(new KnownClass());"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert!(values.output.contains("object(EvalDumpBox)#"));
    assert!(values.output.contains(" (0) {\n}\n"));
    assert!(values.output.contains("object(KnownClass)#"));
    assert_eq!(values.get(result), FakeValue::Null);
}

/// Verifies object dumps include eval-declared properties, dynamic properties, and refs.
#[test]
fn execute_program_var_dump_prints_object_properties_and_references() {
    let program = parse_fragment(
        br#"class EvalDumpProps {
    public $a = 1;
    protected $b = "p";
    private $c = false;
    public function __construct(&$ref) {
        $this->a =& $ref;
        $this->dyn = [2];
    }
}
$x = 9;
$o = new EvalDumpProps($x);
var_dump($o);
$x = 10;
var_dump($o);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert!(values.output.contains("object(EvalDumpProps)#"));
    assert!(values.output.contains(" (4) {\n"));
    assert!(values.output.contains("  [\"a\"]=>\n  &int(9)\n"));
    assert!(values.output.contains("  [\"b\":protected]=>\n  string(1) \"p\"\n"));
    assert!(values.output.contains("  [\"c\":\"EvalDumpProps\":private]=>\n  bool(false)\n"));
    assert!(values.output.contains("  [\"dyn\"]=>\n  array(1) {\n"));
    assert!(values.output.contains("  [\"a\"]=>\n  &int(10)\n"));
    assert_eq!(values.get(result), FakeValue::Null);
}
