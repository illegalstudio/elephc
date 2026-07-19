//! Purpose:
//! Basic interpreter tests for eval-declared construction, inheritance,
//! properties, static dispatch, builtin contracts, and `ArrayAccess`.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover class property semantics that need eval runtime state.

use super::super::super::*;
use super::super::support::*;

/// Verifies promoted constructor properties initialize before the constructor body runs.
#[test]
fn execute_program_initializes_constructor_promoted_properties() {
    let program = parse_fragment(
        br#"class EvalPromotedUser {
    public function __construct(public int $id, private string $name = "Ada") {
        $this->id = $this->id + 1;
    }
    public function label() { return $this->id . ":" . $this->name; }
}
$user = new EvalPromotedUser(6);
echo $user->id; echo ":";
return $user->label();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "7:");
    assert_eq!(values.get(result), FakeValue::String("7:Ada".to_string()));
}

/// Verifies `new self/static/parent` resolve inside eval-declared methods.
#[test]
fn execute_program_constructs_relative_class_names_from_eval_methods() {
    let program = parse_fragment(
        br#"class EvalRelativeFactoryBase {
    public string $label;
    public function __construct($label = "base") { $this->label = $label; }
    public function selfFactory() { return new self("self"); }
    public function staticFactory() { return new static("static"); }
}
class EvalRelativeFactoryChild extends EvalRelativeFactoryBase {
    public function parentFactory() { return new parent("parent"); }
}
$child = new EvalRelativeFactoryChild("root");
$self = $child->selfFactory();
$static = $child->staticFactory();
$parent = $child->parentFactory();
echo get_class($self); echo ":"; echo $self->label; echo ":";
echo get_class($static); echo ":"; echo $static->label; echo ":";
echo get_class($parent); echo ":"; echo $parent->label;
return $self instanceof EvalRelativeFactoryBase
    && !($self instanceof EvalRelativeFactoryChild)
    && $static instanceof EvalRelativeFactoryChild
    && $parent instanceof EvalRelativeFactoryBase
    && !($parent instanceof EvalRelativeFactoryChild);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "EvalRelativeFactoryBase:self:EvalRelativeFactoryChild:static:EvalRelativeFactoryBase:parent"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies `new self/static/parent` stay relative inside eval namespaces.
#[test]
fn execute_program_constructs_namespaced_relative_class_names_from_eval_methods() {
    let program = parse_fragment(
        br#"namespace EvalRelativeNs;
class Base {
    public string $label;
    public function __construct($label = "base") { $this->label = $label; }
    public function selfFactory() { return new self("self"); }
    public function staticFactory() { return new static("static"); }
}
class Child extends Base {
    public function parentFactory() { return new parent("parent"); }
}
$child = new Child("root");
$self = $child->selfFactory();
$static = $child->staticFactory();
$parent = $child->parentFactory();
echo get_class($self); echo ":"; echo $self->label; echo ":";
echo get_class($static); echo ":"; echo $static->label; echo ":";
echo get_class($parent); echo ":"; echo $parent->label;
return $self instanceof Base
    && !($self instanceof Child)
    && $static instanceof Child
    && $parent instanceof Base
    && !($parent instanceof Child);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "EvalRelativeNs\\Base:self:EvalRelativeNs\\Child:static:EvalRelativeNs\\Base:parent"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies PHP legacy `var` properties behave as public eval properties.
#[test]
fn execute_program_supports_legacy_var_properties() {
    let program = parse_fragment(
        br#"trait EvalLegacyVarTrait {
    var ?string $label = "trait";
}
class EvalLegacyVarProperty {
    use EvalLegacyVarTrait;
    var $plain = "p";
    var ?int $count = null;
}
$object = new EvalLegacyVarProperty();
$plain = new ReflectionProperty("EvalLegacyVarProperty", "plain");
$count = new ReflectionProperty("EvalLegacyVarProperty", "count");
$label = new ReflectionProperty("EvalLegacyVarProperty", "label");
$defaults = (new ReflectionClass("EvalLegacyVarProperty"))->getDefaultProperties();
echo $object->plain; echo ":";
echo $plain->isPublic() ? "P" : "p"; echo ":";
echo $plain->hasType() ? "T" : "t"; echo ":";
echo $count->isPublic() ? "C" : "c"; echo ":";
echo $count->hasType() ? $count->getType()->getName() : "none"; echo ":";
echo $count->getType()->allowsNull() ? "N" : "n"; echo ":";
echo is_null($defaults["count"]) ? "null" : "bad"; echo ":";
echo $object->label; echo ":";
echo $label->isPublic() ? "L" : "l"; echo ":";
echo $label->getType()->getName();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "p:P:t:C:int:N:null:trait:L:string");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies comma-separated eval properties initialize instance, static, and trait storage.
#[test]
fn execute_program_reads_comma_separated_eval_properties() {
    let program = parse_fragment(
        br#"class EvalMultiPropertyBox {
    public int $a = 1, $b = 2;
    public static int $s = 3, $t = 4;
    public function sum() { return $this->a + $this->b + self::$s + self::$t; }
}
trait EvalMultiPropertyTrait {
    public int $x = 5, $y = 6;
}
class EvalMultiPropertyTraitBox {
    use EvalMultiPropertyTrait;
    public function sum() { return $this->x + $this->y; }
}
$box = new EvalMultiPropertyBox();
$traitBox = new EvalMultiPropertyTraitBox();
echo $box->a; echo $box->b; echo ":";
echo EvalMultiPropertyBox::$s; echo EvalMultiPropertyBox::$t; echo ":";
echo $traitBox->x; echo $traitBox->y; echo ":";
return $box->sum() + $traitBox->sum();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "12:34:56:");
    assert_eq!(values.get(result), FakeValue::Int(21));
}

/// Verifies eval static method calls preserve PHP late-static forwarding rules.
#[test]
fn execute_program_forwards_eval_static_method_called_class() {
    let program = parse_fragment(
        br#"class EvalForwardA {
    public static function who() { return static::tag(); }
    public static function relayNamed() { return EvalForwardA::who(); }
    public static function relaySelf() { return self::who(); }
    public static function tag() { return "A"; }
}
class EvalForwardB extends EvalForwardA {
    public static function relayParent() { return parent::who(); }
    public static function relayStatic() { return static::who(); }
    public static function tag() { return "B"; }
}
echo EvalForwardB::relayNamed(); echo ":";
echo EvalForwardB::relaySelf(); echo ":";
echo EvalForwardB::relayParent(); echo ":";
return EvalForwardB::relayStatic();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:B:B:");
    assert_eq!(values.get(result), FakeValue::String("B".to_string()));
}

/// Verifies `get_called_class()` follows eval late-static method scopes.
#[test]
fn execute_program_dispatches_get_called_class_builtin() {
    let program = parse_fragment(
        br#"class EvalCalledClassBase {
    public function instanceWho() { return get_called_class(); }
    public function instanceCall() { return call_user_func("get_called_class"); }
    public static function staticWho() { return get_called_class(); }
    public static function staticCallArray() { return call_user_func_array("get_called_class", []); }
    public static function makeCallable() { return get_called_class(...); }
}
class EvalCalledClassChild extends EvalCalledClassBase {}
$child = new EvalCalledClassChild();
echo $child->instanceWho(); echo ":";
echo $child->instanceCall(); echo ":";
echo EvalCalledClassChild::staticWho(); echo ":";
echo EvalCalledClassChild::staticCallArray(); echo ":";
echo EvalCalledClassBase::staticWho(); echo ":";
try {
    get_called_class();
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage(); echo ":";
}
$fn = EvalCalledClassChild::makeCallable();
try {
    $fn();
} catch (Error $e) {
    echo "callable:";
}
echo function_exists("get_called_class"); echo is_callable("get_called_class");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "EvalCalledClassChild:EvalCalledClassChild:EvalCalledClassChild:EvalCalledClassChild:EvalCalledClassBase:Error:get_called_class() must be called from within a class:callable:11"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval classes can extend runtime/AOT classes through the dynamic backing object.
#[test]
fn execute_program_extends_runtime_class_from_eval_declaration() {
    let program = parse_fragment(
        br#"class EvalRuntimeParentChild extends KnownClass {
    public function own() { return $this->read_x() + 1; }
}
$box = new EvalRuntimeParentChild(9);
echo get_class($box); echo ":";
echo get_parent_class($box); echo ":";
echo is_a($box, "EvalRuntimeParentChild") ? "D" : "d"; echo ":";
echo is_a($box, "KnownClass") ? "K" : "k"; echo ":";
echo is_a($box, "KnownInterface") ? "I" : "i"; echo ":";
echo is_subclass_of($box, "KnownClass") ? "S" : "s"; echo ":";
echo is_subclass_of("EvalRuntimeParentChild", "KnownClass") ? "N" : "n"; echo ":";
echo $box->read_x(); echo ":";
return $box->own();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "EvalRuntimeParentChild:KnownClass:D:K:I:S:N:9:"
    );
    assert_eq!(values.get(result), FakeValue::Int(10));
}

/// Verifies eval classes cannot directly implement PHP's special Throwable contract.
#[test]
fn execute_program_rejects_eval_class_implementing_throwable_contracts() {
    for (source, label) in [
        (
            br#"class EvalInvalidThrowableClass implements Throwable {}"# as &[u8],
            "direct Throwable implementation should fail",
        ),
        (
            br#"interface EvalThrowableMarker extends Throwable {}
class EvalInvalidThrowableMarkerClass implements EvalThrowableMarker {}"#,
            "Throwable-derived interface implementation should fail",
        ),
    ] {
        let program = parse_fragment(source).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let err = execute_program(&program, &mut scope, &mut values).expect_err(label);

        assert_eq!(err, EvalStatus::RuntimeFatal);
    }
}

/// Verifies eval classes must satisfy methods required by PHP builtin interfaces.
#[test]
fn execute_program_rejects_invalid_builtin_interface_implementations() {
    for (source, label) in [
        (
            br#"class EvalMissingCountable implements Countable {}"# as &[u8],
            "missing Countable::count should fail",
        ),
        (
            br#"class EvalBadCountableReturn implements Countable {
    public function count(): string { return "1"; }
}"#,
            "incompatible Countable::count return type should fail",
        ),
        (
            br#"class EvalMissingStringable implements Stringable {}"#,
            "missing Stringable::__toString should fail",
        ),
        (
            br#"class EvalBadIterator implements Iterator {
    public function current(): mixed { return null; }
    public function key(): mixed { return null; }
    public function next(): void {}
    public function valid(): bool { return false; }
}"#,
            "missing Iterator::rewind should fail",
        ),
        (
            br#"class EvalBadJsonSerializable implements JsonSerializable {
    public static function jsonSerialize(): mixed { return []; }
}"#,
            "static JsonSerializable::jsonSerialize should fail",
        ),
    ] {
        let program = parse_fragment(source).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let err = execute_program(&program, &mut scope, &mut values).expect_err(label);

        assert_eq!(err, EvalStatus::RuntimeFatal);
    }
}

/// Verifies abstract eval classes can defer PHP builtin interface methods.
#[test]
fn execute_program_allows_abstract_builtin_interface_implementations() {
    let program = parse_fragment(
        br#"abstract class EvalAbstractCountable implements Countable {}
return class_exists("EvalAbstractCountable");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval-declared `ArrayAccess` objects dispatch reads, writes, append, probes, and unset.
#[test]
fn execute_program_dispatches_eval_array_access_objects() {
    let program = parse_fragment(
        br#"class EvalArrayAccessBox implements ArrayAccess {
    public function offsetExists(mixed $offset): bool {
        echo "exists:" . $offset . ":";
        if ($offset === "missing") {
            return false;
        }
        return true;
    }
    public function offsetGet(mixed $offset): mixed {
        echo "get:" . $offset . ":";
        if ($offset === "empty") {
            return "";
        }
        return "v" . $offset;
    }
    public function offsetSet(mixed $offset, mixed $value): void {
        if ($offset === null) {
            echo "set:null:" . $value . ":";
        } else {
            echo "set:" . $offset . ":" . $value . ":";
        }
    }
    public function offsetUnset(mixed $offset): void {
        echo "unset:" . $offset . ":";
    }
}
$box = new EvalArrayAccessBox();
$box["x"] = "1";
$box[] = "tail";
unset($box["drop"]);
if (isset($box["x"])) { echo "I:"; } else { echo "i:"; }
if (isset($box["missing"])) { echo "M:"; } else { echo "m:"; }
if (empty($box["empty"])) { echo "E:"; } else { echo "e:"; }
if (empty($box["missing"])) { echo "N:"; } else { echo "n:"; }
return $box["y"];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "set:x:1:set:null:tail:unset:drop:exists:x:I:exists:missing:m:exists:empty:get:empty:E:exists:missing:N:get:y:"
    );
    assert_eq!(values.get(result), FakeValue::String("vy".to_string()));
}
