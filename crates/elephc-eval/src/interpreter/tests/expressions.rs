//! Purpose:
//! Interpreter tests for scalar expressions, echo/print, objects, and construction.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases assert EvalIR expression execution against fake runtime values.

use super::super::*;
use super::support::*;

/// Verifies simple variable compound assignments read, compute, and write the scope value.
#[test]
fn execute_program_evaluates_compound_assignments() {
    let program =
        parse_fragment(br#"$x = 2; $x += 3; $x *= 4; $x -= 5; $s = "v"; $s .= $x; echo $s;"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.output, "v15");
    assert_eq!(values.get(x), FakeValue::Int(15));
}
/// Verifies division and modulo evaluate through fake runtime numeric hooks.
#[test]
fn execute_program_evaluates_division_and_modulo() {
    let program = parse_fragment(br#"$x = 20; $x /= 2; $x %= 6; echo $x; return 9 / 2;"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.output, "4");
    assert_eq!(values.get(x), FakeValue::Int(4));
    assert_eq!(values.get(result), FakeValue::Float(4.5));
}
/// Verifies exponentiation evaluates through fake runtime numeric hooks.
#[test]
fn execute_program_evaluates_exponentiation() {
    let program = parse_fragment(
        br#"$x = 2; $x **= 3; echo $x; echo ":"; echo -2 ** 2; return 2 ** 3 ** 2;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.output, "8:-4");
    assert_eq!(values.get(x), FakeValue::Float(8.0));
    assert_eq!(values.get(result), FakeValue::Float(512.0));
}
/// Verifies bitwise and shift operators evaluate through fake runtime hooks.
#[test]
fn execute_program_evaluates_bitwise_and_shift_ops() {
    let program = parse_fragment(
        br#"$x = 6; $x &= 3; echo $x; echo ":";
$x = 4; $x |= 1; echo $x; echo ":";
$x = 7; $x ^= 3; echo $x; echo ":";
$x = 1; $x <<= 5; echo $x; echo ":";
$x = 64; $x >>= 3; echo $x; echo ":";
echo ~0; echo ":"; echo -16 >> 2;
return (1 << 4) | ((16 >> 2) ^ (3 & 1));"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:5:4:32:8:-1:-4");
    assert_eq!(values.get(result), FakeValue::Int(21));
}
/// Verifies simple variable increment and decrement statements update the scope value.
#[test]
fn execute_program_evaluates_inc_dec_statements() {
    let program = parse_fragment(br#"$i = 1; $i++; ++$i; $i--; --$i; echo $i;"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let i = scope.visible_cell("i").expect("scope should contain i");

    assert_eq!(values.output, "1");
    assert_eq!(values.get(i), FakeValue::Int(1));
}
/// Verifies echo and unset operate through runtime hooks and scope metadata.
#[test]
fn execute_program_echoes_and_unsets_scope_value() {
    let program =
        parse_fragment(br#"echo "hi" . $name; unset($name);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let name = values.string(" Ada").expect("create fake string");
    scope.set("name", name, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "hi Ada");
    assert_eq!(values.get(result), FakeValue::Null);
    assert!(scope.entry("name").expect("unset marker").flags().unset);
}
/// Verifies comma-separated echo expressions are executed in source order.
#[test]
fn execute_program_echoes_comma_list() {
    let program = parse_fragment(br#"echo "a", $b, "c";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let b = values.string("b").expect("create fake string");
    scope.set("b", b, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "abc");
}
/// Verifies print writes output and returns integer 1.
#[test]
fn execute_program_print_returns_one() {
    let program = parse_fragment(br#"return print "p";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "p");
    assert_eq!(values.get(result), FakeValue::Int(1));
}
/// Verifies eval `print_r()` emits supported values and returns true.
#[test]
fn execute_program_dispatches_print_r_builtin() {
    let program = parse_fragment(
        br#"print_r("x"); echo ":";
print_r(value: false); echo ":";
print_r([1, 2]); echo ":";
$call = call_user_func("print_r", true);
$spread = call_user_func_array("print_r", ["value" => "z"]);
echo ":" . ($call ? "call" : "bad") . ":" . ($spread ? "spread" : "bad") . ":";
return function_exists("print_r");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "x::Array\n:1z:call:spread:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
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
$call = call_user_func("var_dump", 3.5);
$spread = call_user_func_array("var_dump", ["value" => "z"]);
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
            "float(3.5)\n",
            "string(1) \"z\"\n",
            "call-null:spread-null:",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval property reads and writes dispatch through runtime hooks.
#[test]
fn execute_program_reads_and_writes_object_property() {
    let program = parse_fragment(br#"$this->x = $this->x + 1; return $this->x;"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(1).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(2));
    assert_eq!(
        values
            .property_get(object, "x")
            .map(|value| values.get(value))
            .expect("property should be readable"),
        FakeValue::Int(2)
    );
}
/// Verifies eval method calls dispatch through the runtime method hook.
#[test]
fn execute_program_calls_object_method() {
    let program = parse_fragment(br#"return $this->answer();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(42));
}
/// Verifies eval method calls forward evaluated arguments to the runtime hook.
#[test]
fn execute_program_calls_object_method_with_argument() {
    let program = parse_fragment(br#"return $this->add_x(5);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(7).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}
/// Verifies eval method calls forward multiple evaluated arguments to the runtime hook.
#[test]
fn execute_program_calls_object_method_with_two_arguments() {
    let program = parse_fragment(br#"return $this->add2_x(5, 6);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(7).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(18));
}
/// Verifies eval method calls forward numerically unpacked arguments.
#[test]
fn execute_program_calls_object_method_with_spread_arguments() {
    let program =
        parse_fragment(br#"return $this->add2_x(...[5, 6]);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(7).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(18));
}
/// Verifies eval object construction dispatches through runtime hooks.
#[test]
fn execute_program_constructs_named_object() {
    let program = parse_fragment(br#"return new Box();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Object(Vec::new()));
}
/// Verifies eval object construction passes constructor arguments through runtime hooks.
#[test]
fn execute_program_constructs_named_object_with_args() {
    let program = parse_fragment(br#"return new Box(1);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let FakeValue::Object(properties) = values.get(result) else {
        panic!("expected fake object");
    };
    let x = FakeOps::object_property(&properties, "x").expect("constructor should set x");

    assert_eq!(values.get(x), FakeValue::Int(1));
}
/// Verifies eval-declared classes create objects with properties and methods.
#[test]
fn execute_program_constructs_eval_declared_class_with_method() {
    let program = parse_fragment(
        br#"class DynBox {
    public int $x = 1;
    public function __construct($x) { $this->x = $x; }
    public function bump($n) { $this->x = $this->x + $n; return $this->x; }
}
$box = new DynBox(4);
echo get_class($box);
echo ":";
echo $box->bump(3);
echo ":";
echo is_a($box, "DynBox") ? "Y" : "N";
$call = [$box, "bump"];
echo call_user_func($call, 1);
echo ":";
echo call_user_func_array($call, [2]);
echo ":";
return $box->x;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "DynBox:7:Y8:10:");
    assert_eq!(values.get(result), FakeValue::Int(10));
}
/// Verifies eval-declared classes inherit properties, methods, and constructors.
#[test]
fn execute_program_constructs_eval_declared_class_with_inheritance() {
    let program = parse_fragment(
        br#"class EvalBaseBox {
    public int $base = 1;
    public function __construct($base) { $this->base = $base; }
    public function sum($n) { return $this->base + $this->tail + $n; }
}
class EvalChildBox extends EvalBaseBox implements KnownInterface {
    public int $tail = 4;
    public function read($n) { return $this->sum($n); }
}
$box = new EvalChildBox(3);
echo $box->read(5); echo ":";
echo get_parent_class($box); echo ":";
echo is_a($box, "EvalBaseBox") ? "isa" : "bad"; echo ":";
echo is_a($box, "KnownInterface") ? "iface" : "bad"; echo ":";
echo is_subclass_of($box, "EvalChildBox") ? "bad" : "self"; echo ":";
echo is_subclass_of($box, "EvalBaseBox") ? "sub" : "bad"; echo ":";
$parents = class_parents($box);
echo count($parents); echo ":";
echo $parents["EvalBaseBox"];
return $box->base;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "12:EvalBaseBox:isa:iface:self:sub:1:EvalBaseBox"
    );
    assert_eq!(values.get(result), FakeValue::Int(3));
}
/// Verifies eval-declared classes can implement eval-declared interfaces.
#[test]
fn execute_program_constructs_eval_declared_class_with_dynamic_interface() {
    let program = parse_fragment(
        br#"interface EvalReader {
    function read($n);
}
interface EvalNamedReader extends EvalReader {
    function label();
}
class EvalReaderBox implements EvalNamedReader {
    public function read($n) { return $n + 1; }
    public function label() { return "box"; }
}
$box = new EvalReaderBox();
echo $box->read(4); echo ":";
echo $box->label(); echo ":";
echo is_a($box, "EvalNamedReader") ? "isa" : "bad"; echo ":";
echo is_subclass_of($box, "EvalReader") ? "sub" : "bad"; echo ":";
echo is_subclass_of("EvalReaderBox", "EvalReader") ? "str" : "bad"; echo ":";
$implements = class_implements($box);
echo count($implements); echo ":";
echo $implements["EvalNamedReader"]; echo ":";
echo $implements["EvalReader"];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "5:box:isa:sub:str:2:EvalNamedReader:EvalReader"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies concrete eval classes can implement abstract class and interface contracts.
#[test]
fn execute_program_constructs_concrete_child_from_abstract_eval_class() {
    let program = parse_fragment(
        br#"interface EvalAbstractReadable {
    function read($n);
}
abstract class EvalAbstractBase implements EvalAbstractReadable {
    abstract public function read($n);
    public function wrap($n) { return $this->read($n) + 1; }
}
class EvalConcreteBox extends EvalAbstractBase {
    public function read($n) { return $n + 3; }
}
$box = new EvalConcreteBox();
echo $box->wrap(4); echo ":";
echo is_a($box, "EvalAbstractReadable") ? "iface" : "bad"; echo ":";
echo is_subclass_of($box, "EvalAbstractBase") ? "abstract" : "bad";
return $box->read(2);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "8:iface:abstract");
    assert_eq!(values.get(result), FakeValue::Int(5));
}
/// Verifies eval rejects instantiation of abstract eval-declared classes.
#[test]
fn execute_program_rejects_abstract_eval_class_instantiation() {
    let program = parse_fragment(
        br#"abstract class EvalAbstractOnly {
    public function read() { return 1; }
}
new EvalAbstractOnly();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("abstract class instantiation should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
/// Verifies concrete eval classes must implement inherited abstract methods.
#[test]
fn execute_program_rejects_concrete_eval_class_with_abstract_methods() {
    let program = parse_fragment(
        br#"abstract class EvalNeedsRead {
    abstract public function read();
}
class EvalMissingReadChild extends EvalNeedsRead {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("concrete class missing abstract method should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
/// Verifies eval rejects extending a final eval-declared class.
#[test]
fn execute_program_rejects_extending_final_eval_class() {
    let program = parse_fragment(
        br#"final class EvalFinalBase {}
class EvalFinalChild extends EvalFinalBase {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("extending final class should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
/// Verifies eval rejects overriding a final eval-declared method.
#[test]
fn execute_program_rejects_overriding_final_eval_method() {
    let program = parse_fragment(
        br#"class EvalFinalMethodBase {
    final public function read() { return 1; }
}
class EvalFinalMethodChild extends EvalFinalMethodBase {
    public function read() { return 2; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("overriding final method should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
/// Verifies eval-declared traits contribute methods, properties, and metadata.
#[test]
fn execute_program_constructs_class_using_eval_declared_trait() {
    let program = parse_fragment(
        br#"trait EvalReusableTrait {
    public int $seed = 2;
    public function add($n) { return $this->seed + $n; }
}
class EvalTraitBox {
    use EvalReusableTrait;
    public function read($n) { return $this->add($n) + 1; }
}
$box = new EvalTraitBox();
echo $box->read(4); echo ":";
echo trait_exists("EvalReusableTrait") ? "trait" : "bad"; echo ":";
$traits = get_declared_traits();
echo count($traits); echo ":"; echo $traits[0]; echo ":";
$uses = class_uses($box);
echo count($uses); echo ":"; echo $uses["EvalReusableTrait"];
return $box->seed;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "7:trait:1:EvalReusableTrait:1:EvalReusableTrait"
    );
    assert_eq!(values.get(result), FakeValue::Int(2));
}
/// Verifies eval trait abstract methods can be implemented by the using class.
#[test]
fn execute_program_constructs_class_satisfying_eval_trait_abstract_method() {
    let program = parse_fragment(
        br#"trait EvalTraitNeedsRead {
    abstract public function read($n);
    public function wrap($n) { return $this->read($n) + 1; }
}
class EvalTraitReader {
    use EvalTraitNeedsRead;
    public function read($n) { return $n + 4; }
}
$reader = new EvalTraitReader();
return $reader->wrap(3);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(8));
}
/// Verifies eval rejects a concrete class that leaves a trait abstract method open.
#[test]
fn execute_program_rejects_missing_eval_trait_abstract_method() {
    let program = parse_fragment(
        br#"trait EvalTraitAbstractMethod {
    abstract public function read();
}
class EvalTraitMissingRead {
    use EvalTraitAbstractMethod;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("class missing trait abstract method should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
/// Verifies eval rejects classes using traits that are not eval-declared.
#[test]
fn execute_program_rejects_missing_eval_trait_use() {
    let program = parse_fragment(
        br#"class EvalTraitMissingUse {
    use MissingEvalTraitUse;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("missing eval trait use should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
/// Verifies eval rejects classes missing methods required by eval interfaces.
#[test]
fn execute_program_rejects_missing_dynamic_interface_method() {
    let program = parse_fragment(
        br#"interface EvalNeedsRead {
    function read($n);
}
class EvalMissingRead implements EvalNeedsRead {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("missing interface method should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
