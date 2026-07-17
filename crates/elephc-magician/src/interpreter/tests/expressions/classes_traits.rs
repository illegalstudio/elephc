//! Purpose:
//! Interpreter tests for eval class construction, inheritance, abstract/final
//! rules, dynamic interfaces, and traits.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Class declaration validation and construction behavior are exercised together.

use super::super::super::*;
use super::super::support::*;

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

/// Verifies eval `instanceof` uses eval class, interface, and dynamic-target metadata.
#[test]
fn execute_program_evaluates_eval_instanceof_targets() {
    let program = parse_fragment(
        br#"interface EvalInstanceIface {}
class EvalInstanceBase {}
class EvalInstanceChild extends EvalInstanceBase implements EvalInstanceIface {}
class EvalInstanceOther {}
$box = new EvalInstanceChild();
$class = "EvalInstanceChild";
$target = ["EvalInstanceIface"];
$prefix = "EvalInstance";
$suffix = "Base";
$targetObject = new EvalInstanceChild();
echo $box instanceof EvalInstanceChild ? "C" : "c";
echo $box instanceof EvalInstanceBase ? "B" : "b";
echo $box instanceof EvalInstanceIface ? "I" : "i";
echo $box instanceof $class ? "D" : "d";
echo $box instanceof $target[0] ? "A" : "a";
echo $box instanceof ($prefix . $suffix) ? "P" : "p";
echo $box instanceof $targetObject ? "O" : "o";
echo 7 instanceof MissingEvalClass ? "bad" : "S";
return $box instanceof EvalInstanceOther;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "CBIDAPOS");
    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// Verifies dynamic `instanceof` rejects targets that are not strings or objects.
#[test]
fn execute_program_rejects_invalid_dynamic_instanceof_target() {
    let program = parse_fragment(
        br#"class EvalInvalidInstanceTarget {}
$box = new EvalInvalidInstanceTarget();
$target = 42;
return $box instanceof $target;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("invalid instanceof target should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
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

/// Verifies eval rejects overriding a final eval-declared property.
#[test]
fn execute_program_rejects_overriding_final_eval_property() {
    let program = parse_fragment(
        br#"class EvalFinalPropertyBase {
    final public $value = 1;
}
class EvalFinalPropertyChild extends EvalFinalPropertyBase {
    public $value = 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("overriding final property should fail");

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
