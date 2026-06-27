//! Purpose:
//! Interpreter tests for eval-declared class runtime behavior.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover class property semantics that need eval runtime state.

use super::super::*;
use super::support::*;

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

/// Verifies promoted readonly properties throw Error outside their constructor.
#[test]
fn execute_program_promoted_readonly_property_write_after_constructor_throws_error() {
    let program = parse_fragment(
        br#"class EvalPromotedReadonlyBox {
    public function __construct(public readonly int $id) {}
    public function replace($id) { $this->id = $id; }
}
$box = new EvalPromotedReadonlyBox(7);
echo $box->id;
try {
    $box->replace(8);
    echo "bad";
} catch (Error $e) {
    echo ":"; echo get_class($e); echo ":"; echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "7:Error:Cannot modify readonly property EvalPromotedReadonlyBox::$id"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

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

/// Verifies direct reads of uninitialized typed eval properties throw catchable PHP errors.
#[test]
fn execute_program_rejects_uninitialized_typed_property_reads() {
    let program = parse_fragment(
        br#"class EvalTypedReadBox {
    public int $typed;
    public ?int $nullable;
    public ?int $defaultNull = null;
    public $plain;
}
$box = new EvalTypedReadBox();
try {
    echo $box->typed;
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    echo $box->nullable;
} catch (Error $e) {
    echo $e->getMessage();
}
echo "|";
echo is_null($box->defaultNull) ? "default-null" : "bad";
echo "|";
echo is_null($box->plain) ? "plain-null" : "bad";
echo "|";
$box->typed = 0;
echo $box->typed;
echo "|";
unset($box->typed);
try {
    echo $box->typed;
} catch (Error $e) {
    echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Typed property EvalTypedReadBox::$typed must not be accessed before initialization|\
Typed property EvalTypedReadBox::$nullable must not be accessed before initialization|\
default-null|plain-null|0|\
Typed property EvalTypedReadBox::$typed must not be accessed before initialization"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly eval properties throw Error on writes outside the declaring constructor.
#[test]
fn execute_program_readonly_property_write_after_constructor_throws_error() {
    let program = parse_fragment(
        br#"class EvalReadonlyBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyBox(7);
try {
    $box->replace(8);
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
        "Error:Cannot modify readonly property EvalReadonlyBox::$id"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly eval properties must declare a type like PHP requires.
#[test]
fn execute_program_rejects_untyped_readonly_properties() {
    let explicit = parse_fragment(
        br#"class EvalReadonlyUntypedBox {
    public readonly $value;
}"#,
    )
    .expect("parse explicit readonly property");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&explicit, &mut scope, &mut values)
        .expect_err("explicit readonly property without type should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);

    let readonly_class = parse_fragment(
        br#"readonly class EvalReadonlyClassUntypedBox {
    public $value;
}"#,
    )
    .expect("parse readonly class property");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&readonly_class, &mut scope, &mut values)
        .expect_err("readonly class property without type should fail");

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

/// Verifies readonly class instance properties throw Error on writes after construction.
#[test]
fn execute_program_readonly_class_property_write_after_constructor_throws_error() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyClassFailBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyClassFailBox(11);
try {
    $box->replace(12);
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
        "Error:Cannot modify readonly property EvalReadonlyClassFailBox::$id"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly classes throw Error on dynamic property creation without a magic setter.
#[test]
fn execute_program_readonly_class_dynamic_property_creation_throws_error() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyDynamicFailBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
}
$box = new EvalReadonlyDynamicFailBox(11);
try {
    $box->dynamic = 12;
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
        "Error:Cannot create dynamic property EvalReadonlyDynamicFailBox::$dynamic"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly classes may still handle missing property writes through `__set()`.
#[test]
fn execute_program_allows_readonly_class_magic_set_for_missing_properties() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyMagicSetBox {
    public function __set($name, $value) {
        echo $name; echo ":"; echo $value;
    }
}
$box = new EvalReadonlyMagicSetBox();
$box->dynamic = 12;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "dynamic:12");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly classes reject PHP's global dynamic-property marker attribute.
#[test]
fn execute_program_rejects_allow_dynamic_properties_on_readonly_class() {
    let program =
        parse_fragment(br#"#[\AllowDynamicProperties] readonly class EvalReadonlyAllowDynamic {}"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("AllowDynamicProperties cannot apply to readonly classes");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies namespaced non-builtin attributes do not trigger the readonly-class marker rule.
#[test]
fn execute_program_allows_namespaced_allow_dynamic_properties_on_readonly_class() {
    let program = parse_fragment(
        br#"namespace EvalReadonlyAttrNs;
#[AllowDynamicProperties] readonly class Box {}
echo class_attribute_names("EvalReadonlyAttrNs\Box")[0];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "EvalReadonlyAttrNs\\AllowDynamicProperties");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval validates PHP's global `#[Override]` method marker.
#[test]
fn execute_program_validates_override_attribute_targets() {
    let valid = parse_fragment(
        br#"interface EvalOverrideContract {
    public function label(): string;
}
class EvalOverrideBase {
    public function name(): string { return "base"; }
}
class EvalOverrideChild extends EvalOverrideBase implements EvalOverrideContract {
    #[\Override]
    public function name(): string { return "child"; }
    #[Override]
    public function label(): string { return "contract"; }
}
$box = new EvalOverrideChild();
echo $box->name() . ":" . $box->label();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&valid, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "child:contract");

    let invalid = parse_fragment(
        br#"class EvalOverrideMissing {
    #[\Override]
    public function missing(): string { return "bad"; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&invalid, &mut scope, &mut values)
        .expect_err("override marker without target should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies readonly classes leave static properties mutable like ordinary classes.
#[test]
fn execute_program_allows_readonly_class_static_property() {
    let program = parse_fragment(
        br#"readonly class EvalReadonlyStaticBox {
    public static int $count = 1;
}
EvalReadonlyStaticBox::$count = EvalReadonlyStaticBox::$count + 1;
echo EvalReadonlyStaticBox::$count;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2");
    assert_eq!(values.get(result), FakeValue::Null);
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

/// Verifies anonymous eval classes instantiate, reuse their synthetic class, and reflect as anonymous.
#[test]
fn execute_program_instantiates_anonymous_class_expressions() {
    let program = parse_fragment(
        br#"interface EvalAnonRuntimeLabel {
    function label();
}
class EvalAnonRuntimeBase {
    protected string $prefix;
    public function __construct($prefix) { $this->prefix = $prefix; }
}
function eval_anon_make($prefix) {
    return new class($prefix) extends EvalAnonRuntimeBase implements EvalAnonRuntimeLabel {
        public function label() { return $this->prefix . ":anon"; }
    };
}
$first = eval_anon_make("A");
$second = eval_anon_make("B");
echo $first->label(); echo ":";
echo $second->label(); echo ":";
echo get_class($first) === get_class($second) ? "same" : "different"; echo ":";
$ref = new ReflectionClass(get_class($first));
echo $ref->isAnonymous() ? "anonymous" : "named"; echo ":";
echo $ref->implementsInterface("EvalAnonRuntimeLabel") ? "iface" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:anon:B:anon:same:anonymous:iface");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies readonly anonymous eval classes initialize and reject property writes.
#[test]
fn execute_program_instantiates_readonly_anonymous_class_expressions() {
    let program = parse_fragment(
        br#"$box = new readonly class("frozen") {
    public function __construct(public string $label) {}
};
echo $box->label; echo ":";
try {
    $box->label = "bad";
    echo "bad";
} catch (Error $e) {
    echo get_class($e);
}
return $box->label;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "frozen:Error");
    assert_eq!(values.get(result), FakeValue::String("frozen".to_string()));
}

/// Verifies eval object cloning copies properties before running `__clone()`.
#[test]
fn execute_program_clones_eval_object_and_runs_clone_hook() {
    let program = parse_fragment(
        br#"class EvalCloneRuntimeBox {
    public string $name;
    public function __construct($name) { $this->name = $name; }
    public function __clone() { $this->name = $this->name . ":clone"; }
}
$first = new EvalCloneRuntimeBox("A");
$second = clone $first;
echo $first->name; echo ":";
echo $second->name;
$second->name = "B";
return $first->name . ":" . $second->name;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:A:clone");
    assert_eq!(values.get(result), FakeValue::String("A:B".to_string()));
}

/// Verifies private `__clone()` can be invoked from inside the declaring eval class.
#[test]
fn execute_program_allows_private_clone_hook_inside_declaring_class() {
    let program = parse_fragment(
        br#"class EvalCloneRuntimePrivateBox {
    public string $name = "A";
    private function __clone() { $this->name = $this->name . ":copy"; }
    public function copy() { return clone $this; }
}
$first = new EvalCloneRuntimePrivateBox();
$second = $first->copy();
echo $first->name; echo ":";
echo $second->name;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:A:copy");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval-declared `__destruct()` runs for explicit unset and discarded temporaries.
#[test]
fn execute_program_runs_eval_destructor_on_final_release() {
    let program = parse_fragment(
        br#"class EvalDestructRuntimeBox {
    public string $name;
    public function __construct($name) { $this->name = $name; }
    public function __destruct() { echo "drop:" . $this->name . ":"; }
}
$box = new EvalDestructRuntimeBox("A");
unset($box);
new EvalDestructRuntimeBox("B");
echo "after";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "drop:A:drop:B:after");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies private `__clone()` throws Error through a global clone expression.
#[test]
fn execute_program_private_clone_hook_outside_declaring_class_throws_error() {
    let program = parse_fragment(
        br#"class EvalCloneRuntimePrivateFail {
    private function __clone() {}
}
$box = new EvalCloneRuntimePrivateFail();
try {
    clone $box;
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
        "Error:Call to private EvalCloneRuntimePrivateFail::__clone() from global scope"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies a get-only property hook computes a virtual eval property.
#[test]
fn execute_program_reads_eval_property_get_hook() {
    let program = parse_fragment(
        br#"class EvalHookPerson {
    public string $first = "Ada";
    public string $last = "Lovelace";
    public string $full {
        get => $this->first . " " . $this->last;
    }
}
$person = new EvalHookPerson();
echo $person->full;
return $person->full;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada Lovelace");
    assert_eq!(
        values.get(result),
        FakeValue::String("Ada Lovelace".to_string())
    );
}

/// Verifies by-reference get hook syntax routes through the concrete eval get accessor.
#[test]
fn execute_program_reads_eval_by_ref_get_property_hook() {
    let program = parse_fragment(
        br#"class EvalByRefGetHookPerson {
    public string $first = "Ada";
    public string $last = "Lovelace";
    public string $full {
        &get => $this->first . " " . $this->last;
    }
}
$person = new EvalByRefGetHookPerson();
echo $person->full;
return $person->full;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada Lovelace");
    assert_eq!(
        values.get(result),
        FakeValue::String("Ada Lovelace".to_string())
    );
}

/// Verifies get/set property hooks can use the raw backing slot from inside accessors.
#[test]
fn execute_program_routes_eval_property_get_and_set_hooks() {
    let program = parse_fragment(
        br#"class EvalHookName {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
$name = new EvalHookName();
$name->value = "Ada";
echo $name->value;
return $name->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada!");
    assert_eq!(values.get(result), FakeValue::String("Ada!".to_string()));
}

/// Verifies short set hooks assign their expression result into the raw backing slot.
#[test]
fn execute_program_routes_eval_short_set_property_hooks() {
    let program = parse_fragment(
        br#"class EvalShortSetHookName {
    public string $value {
        get => $this->value;
        set => trim($value);
    }
}
class EvalShortSetHookLabel {
    public string $text {
        get => $this->text;
        set(string $raw) => strtoupper($raw);
    }
}
$name = new EvalShortSetHookName();
$name->value = "  Ada  ";
echo "[" . $name->value . "]:";
$label = new EvalShortSetHookLabel();
$label->text = "hi";
echo $label->text;
return $label->text;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "[Ada]:HI");
    assert_eq!(values.get(result), FakeValue::String("HI".to_string()));
}

/// Verifies explicit set-hook parameter types are contravariant with the property type.
#[test]
fn execute_program_validates_eval_property_set_hook_parameter_types() {
    let valid_program = parse_fragment(
        br#"class EvalWideSetHookParam {
    public string $value {
        get => $this->value;
        set(mixed $raw) => $raw;
    }
}
$box = new EvalWideSetHookParam();
$box->value = "Ada";
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&valid_program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("Ada".to_string()));

    for source in [
        br#"class EvalNarrowSetHookParam {
    public mixed $value {
        set(string $raw) => $raw;
    }
}"#
        .as_slice(),
        br#"class EvalNullableSetHookParam {
    public ?string $value {
        set(string $raw) => $raw;
    }
}"#
        .as_slice(),
    ] {
        let program = parse_fragment(source).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let err = execute_program(&program, &mut scope, &mut values)
            .expect_err("incompatible set-hook parameter type should fail");
        assert_eq!(err, EvalStatus::RuntimeFatal);
    }
}

/// Verifies nullsafe reads and mixed-case names still route through eval property hooks.
#[test]
fn execute_program_routes_eval_nullsafe_and_mixed_case_property_hooks() {
    let program = parse_fragment(
        br#"class EvalNullsafeHookPerson {
    public string $first = "Ada";
    public string $last = "Lovelace";
    public string $full {
        get => $this->first . " " . $this->last;
    }
}
class EvalMixedCaseHookBox {
    private int $store = 0;
    public int $Total {
        get { return $this->store; }
    }
    public function set(int $value) { $this->store = $value; }
}
function eval_hook_describe($person) {
    return $person?->full ?? "(none)";
}
$person = new EvalNullsafeHookPerson();
$box = new EvalMixedCaseHookBox();
$box->set(5);
echo eval_hook_describe($person) . "|" . eval_hook_describe(null) . "|" . $box->Total;
return $box->Total;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada Lovelace|(none)|5");
    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies undefined eval property reads and writes dispatch through `__get` and `__set`.
#[test]
fn execute_program_dispatches_eval_magic_get_and_set() {
    let program = parse_fragment(
        br#"class EvalMagicPropertyBox {
    public string $events = "";
    public function __get($name) {
        $this->events = $this->events . "get:" . $name . ";";
        return "value:" . $name;
    }
    public function __set($name, $value) {
        $this->events = $this->events . "set:" . $name . "=" . $value . ";";
    }
}
$box = new EvalMagicPropertyBox();
echo $box->missing; echo ":";
$box->other = "B";
$box->events = $box->events . "public;";
return $box->events;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "value:missing:");
    assert_eq!(
        values.get(result),
        FakeValue::String("get:missing;set:other=B;public;".to_string())
    );
}

/// Verifies eval invokes non-public magic methods that PHP accepts with warnings.
#[test]
fn execute_program_dispatches_non_public_eval_magic_methods() {
    let program = parse_fragment(
        br#"class EvalNonPublicMagicBox {
    public string $events = "";
    protected function __get(string $name) {
        $this->events = $this->events . "get:" . $name . ";";
        return "value:" . $name;
    }
    protected function __set(string $name, $value): void {
        $this->events = $this->events . "set:" . $name . "=" . $value . ";";
    }
    private function __isset(string $name): bool {
        $this->events = $this->events . "isset:" . $name . ";";
        return true;
    }
    private function __unset(string $name): void {
        $this->events = $this->events . "unset:" . $name . ";";
    }
    private function __call(string $name, array $args) {
        return $name . ":" . $args[0] . ":" . $args["name"];
    }
    private static function __callStatic(string $name, array $args) {
        return $name . ":" . $args[0] . ":" . $args["name"];
    }
    private function __invoke(string $left = "I", string $right = "J") {
        return "invoke:" . $left . $right;
    }
}
$box = new EvalNonPublicMagicBox();
echo is_callable($box) ? "callable:" : "bad:";
echo $box->missing; echo ":";
$box->other = "B";
echo isset($box->probe) ? "isset:" : "bad:";
unset($box->gone);
echo $box->run("A", name: "B"); echo ":";
echo EvalNonPublicMagicBox::staticRun("C", name: "D"); echo ":";
echo $box(right: "F", left: "E");
return $box->events;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "callable:value:missing:isset:run:A:B:staticRun:C:D:invoke:EF"
    );
    assert_eq!(
        values.get(result),
        FakeValue::String("get:missing;set:other=B;isset:probe;unset:gone;".to_string())
    );
}

/// Verifies inaccessible eval properties dispatch through magic property methods.
#[test]
fn execute_program_dispatches_inaccessible_eval_properties_to_magic_methods() {
    let program = parse_fragment(
        br#"class EvalMagicPrivatePropertyBox {
    private string $secret = "raw";
    public string $events = "";
    public function readOwn() { return $this->secret; }
    public function __get($name) {
        $this->events = $this->events . "get:" . $name . ";";
        return "read:" . $name;
    }
    public function __set($name, $value) {
        $this->events = $this->events . "set:" . $name . "=" . $value . ";";
    }
}
$box = new EvalMagicPrivatePropertyBox();
echo $box->readOwn(); echo ":";
echo $box->secret; echo ":";
$box->secret = "new";
return $box->events;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "raw:read:secret:");
    assert_eq!(
        values.get(result),
        FakeValue::String("get:secret;set:secret=new;".to_string())
    );
}

/// Verifies dynamic properties created without `__set` are read directly even when `__get` exists.
#[test]
fn execute_program_reads_existing_dynamic_property_before_magic_get() {
    let program = parse_fragment(
        br#"class EvalMagicExistingDynamicBox {
    public function __get($name) {
        return "magic:" . $name;
    }
}
$box = new EvalMagicExistingDynamicBox();
$box->known = "plain";
echo $box->known; echo ":";
return $box->missing;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "plain:");
    assert_eq!(
        values.get(result),
        FakeValue::String("magic:missing".to_string())
    );
}

/// Verifies eval property probes and unsets dispatch through `__isset` and `__unset`.
#[test]
fn execute_program_dispatches_eval_magic_isset_empty_and_unset() {
    let program = parse_fragment(
        br#"class EvalMagicPropertyProbeBox {
    public string $events = "";
    public string $present = "ready";
    public $nullish = null;
    private string $secret = "raw";
    public function __isset($name) {
        $this->events = $this->events . "isset:" . $name . ";";
        return $name !== "no";
    }
    public function __get($name) {
        $this->events = $this->events . "get:" . $name . ";";
        return $name === "empty" ? "" : "value:" . $name;
    }
    public function __unset($name) {
        $this->events = $this->events . "unset:" . $name . ";";
    }
}
$box = new EvalMagicPropertyProbeBox();
echo isset($box->present) ? "P" : "p"; echo ":";
echo isset($box->nullish) ? "N" : "n"; echo ":";
echo isset($box->secret) ? "S" : "s"; echo ":";
echo isset($box->no) ? "bad" : "no"; echo ":";
echo empty($box->secret) ? "bad" : "filled"; echo ":";
echo empty($box->empty) ? "empty" : "bad"; echo ":";
unset($box->present);
unset($box->secret);
unset($box->missing);
echo isset($box->present) ? "bad" : "unset"; echo ":";
return $box->events;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "P:n:S:no:filled:empty:unset:");
    assert_eq!(
        values.get(result),
        FakeValue::String(
            "isset:secret;isset:no;isset:secret;get:secret;isset:empty;get:empty;unset:secret;unset:missing;"
                .to_string()
        )
    );
}

/// Verifies eval objects stringify through public `__toString()` in PHP string contexts.
#[test]
fn execute_program_dispatches_eval_magic_tostring_for_string_contexts() {
    let program = parse_fragment(
        br#"class EvalStringableBox {
    public string $name = "Ada";
    public function __toString() {
        return "box:" . $this->name;
    }
    public function accepts(string $value) {
        return "typed:" . $value;
    }
}
$box = new EvalStringableBox();
echo $box; echo ":";
print $box; echo ":";
echo "pre" . $box; echo ":";
echo strval($box); echo ":";
echo call_user_func("strval", $box); echo ":";
echo call_user_func_array("strval", [$box]); echo ":";
echo $box instanceof Stringable ? "S" : "s"; echo ":";
return $box->accepts($box);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "box:Ada:box:Ada:prebox:Ada:box:Ada:box:Ada:box:Ada:S:"
    );
    assert_eq!(
        values.get(result),
        FakeValue::String("typed:box:Ada".to_string())
    );
}

/// Verifies eval objects without `__toString()` fail in PHP string contexts.
#[test]
fn execute_program_rejects_eval_object_string_context_without_tostring() {
    let program = parse_fragment(
        br#"class EvalPlainStringContext {}
$box = new EvalPlainStringContext();
try {
    echo $box;
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Object of class EvalPlainStringContext could not be converted to string"
    );
}

/// Verifies eval rejects magic methods whose staticness, arity, or fatal contracts are invalid.
#[test]
fn execute_program_rejects_invalid_eval_magic_method_contracts() {
    let cases: Vec<(&[u8], &str)> = vec![
        (
            br#"class EvalBadToString { private function __toString() { return "x"; } }"#.as_slice(),
            "private __toString",
        ),
        (
            br#"class EvalBadToStringReturn { public function __toString(): int { return 1; } }"#.as_slice(),
            "bad __toString return type",
        ),
        (
            br#"class EvalBadGetByRef { public function __get(&$name) { return "x"; } }"#.as_slice(),
            "by-ref __get",
        ),
        (
            br#"class EvalBadGetParamType { public function __get(int $name) { return "x"; } }"#.as_slice(),
            "bad __get parameter type",
        ),
        (
            br#"class EvalBadIssetReturn { public function __isset($name): string { return "yes"; } }"#.as_slice(),
            "bad __isset return type",
        ),
        (
            br#"class EvalBadUnsetReturn { public function __unset($name): int { return 1; } }"#.as_slice(),
            "bad __unset return type",
        ),
        (
            br#"class EvalBadSetReturn { public function __set($name, $value): int { return 1; } }"#.as_slice(),
            "bad __set return type",
        ),
        (
            br#"class EvalBadSetParamType { public function __set(int $name, $value): void {} }"#.as_slice(),
            "bad __set parameter type",
        ),
        (
            br#"class EvalBadCall { public function __call($name, ...$args) { return "x"; } }"#.as_slice(),
            "variadic __call",
        ),
        (
            br#"class EvalBadCallArgsType { public function __call(string $name, string $args) {} }"#.as_slice(),
            "bad __call args type",
        ),
        (
            br#"class EvalBadCallNameType { public function __call(int $name, array $args) {} }"#.as_slice(),
            "bad __call name type",
        ),
        (
            br#"class EvalBadCallStatic { public function __callStatic($name, $args) { return "x"; } }"#.as_slice(),
            "instance __callStatic",
        ),
        (
            br#"class EvalBadCallStaticArgsType { public static function __callStatic(string $name, string $args) {} }"#.as_slice(),
            "bad __callStatic args type",
        ),
        (
            br#"class EvalBadSleepReturn { public function __sleep(): string { return "x"; } }"#.as_slice(),
            "bad __sleep return type",
        ),
        (
            br#"class EvalBadSerializeStatic { public static function __serialize(): array { return []; } }"#.as_slice(),
            "static __serialize",
        ),
        (
            br#"class EvalBadWakeupArity { public function __wakeup($value): void {} }"#.as_slice(),
            "bad __wakeup arity",
        ),
        (
            br#"class EvalBadUnserializeArity { public function __unserialize(): void {} }"#.as_slice(),
            "bad __unserialize arity",
        ),
        (
            br#"class EvalBadUnserializeReturn { public function __unserialize(array $data): int { return 1; } }"#.as_slice(),
            "bad __unserialize return type",
        ),
        (
            br#"class EvalBadUnserializeParam { public function __unserialize(string $data): void {} }"#.as_slice(),
            "bad __unserialize parameter type",
        ),
        (
            br#"class EvalBadDebugInfoReturn { public function __debugInfo(): string { return "x"; } }"#.as_slice(),
            "bad __debugInfo return type",
        ),
        (
            br#"class EvalBadDebugInfoStatic { public static function __debugInfo(): array { return []; } }"#.as_slice(),
            "static __debugInfo",
        ),
        (
            br#"class EvalBadSetStateInstance { public function __set_state($data) {} }"#.as_slice(),
            "instance __set_state",
        ),
        (
            br#"class EvalBadSetStateArity { public static function __set_state($data, $extra) {} }"#.as_slice(),
            "bad __set_state arity",
        ),
        (
            br#"class EvalBadSetStateParam { public static function __set_state(string $data) {} }"#.as_slice(),
            "bad __set_state parameter type",
        ),
        (
            br#"class EvalBadClone { public static function __clone() {} }"#.as_slice(),
            "static __clone",
        ),
        (
            br#"class EvalBadCloneReturn { public function __clone(): int {} }"#.as_slice(),
            "bad __clone return type",
        ),
        (
            br#"class EvalBadDestruct { public static function __destruct() {} }"#.as_slice(),
            "static __destruct",
        ),
        (
            br#"class EvalBadConstructReturn { public function __construct(): void {} }"#.as_slice(),
            "bad __construct return type",
        ),
        (
            br#"class EvalBadDestructReturn { public function __destruct(): void {} }"#.as_slice(),
            "bad __destruct return type",
        ),
        (
            br#"trait EvalBadMagicTrait { public static function __isset($name) { return true; } }"#.as_slice(),
            "trait static __isset",
        ),
    ];

    for (source, label) in cases {
        let program = parse_fragment(source).expect(label);
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        execute_program(&program, &mut scope, &mut values).expect_err(label);
    }
}

/// Verifies eval accepts PHP-compatible debug and set-state magic method contracts.
#[test]
fn execute_program_accepts_debug_and_set_state_magic_contracts() {
    let program = parse_fragment(
        br#"class EvalGoodDebugInfoMagic {
    public function __debugInfo(): ?array { return null; }
}
class EvalGoodSetStateMagic {
    public static function __set_state($data) {}
}
return class_exists("EvalGoodDebugInfoMagic") && class_exists("EvalGoodSetStateMagic");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies get-only property hooks throw Error on writes outside a set accessor.
#[test]
fn execute_program_write_to_get_only_eval_property_hook_throws_error() {
    let program = parse_fragment(
        br#"class EvalHookReadOnly {
    public int $answer {
        get => 42;
    }
}
$box = new EvalHookReadOnly();
try {
    $box->answer = 7;
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
        "Error:Property EvalHookReadOnly::$answer is read-only"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval subclasses inherit parent property hooks.
#[test]
fn execute_program_inherits_eval_property_hooks() {
    let program = parse_fragment(
        br#"class EvalHookBase {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
class EvalHookChild extends EvalHookBase {
    public function shout() { return $this->value . "?"; }
}
$box = new EvalHookChild();
$box->value = "Ada";
echo $box->value; echo ":";
return $box->shout();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada!:");
    assert_eq!(values.get(result), FakeValue::String("Ada!?".to_string()));
}

/// Verifies eval interface property hook contracts are enforced through inheritance.
#[test]
fn execute_program_accepts_interface_property_hook_contracts() {
    let program = parse_fragment(
        br#"interface EvalHookContract {
    public string $value { get; set; }
}
interface EvalNamedHookContract extends EvalHookContract {
    public string $name { get; }
}
class EvalHookContractBox implements EvalNamedHookContract {
    public string $name = "box";
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
$box = new EvalHookContractBox();
$box->value = "Ada";
echo $box->name; echo ":";
echo $box->value;
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "box:Ada!");
    assert_eq!(values.get(result), FakeValue::String("Ada!".to_string()));
}

/// Verifies a normal public mutable property satisfies an eval interface get/set contract.
#[test]
fn execute_program_accepts_plain_property_for_interface_hook_contracts() {
    let program = parse_fragment(
        br#"interface EvalPlainHookContract {
    public string $value { get; set; }
}
class EvalPlainHookContractBox implements EvalPlainHookContract {
    public string $value = "Ada";
}
$box = new EvalPlainHookContractBox();
echo $box->value; echo ":";
$box->value = "Grace";
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada:");
    assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
}

/// Verifies interface property hook types are checked on abstract and concrete classes.
#[test]
fn execute_program_validates_interface_property_hook_types() {
    let valid_program = parse_fragment(
        br#"interface EvalIfaceGetWide {
    public int|string $value { get; }
}
interface EvalIfaceSetNarrow {
    public int $slot { set; }
}
abstract class EvalIfacePropertyDeferred implements EvalIfaceGetWide {}
abstract class EvalIfacePropertyGood implements EvalIfaceGetWide, EvalIfaceSetNarrow {
    abstract public int $value { get; }
    abstract public int|string $slot { set; }
}
class EvalIfacePropertyConcrete implements EvalIfaceGetWide {
    public int $value = 4;
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&valid_program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.get(result), FakeValue::Bool(true));

    let bad_abstract_get = parse_fragment(
        br#"interface EvalIfaceGetInt {
    public int $value { get; }
}
abstract class EvalIfaceGetWideBad implements EvalIfaceGetInt {
    abstract public int|string $value { get; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_abstract_get, &mut scope, &mut values)
        .expect_err("wider abstract get property type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let bad_abstract_set = parse_fragment(
        br#"interface EvalIfaceSetWide {
    public int|string $value { set; }
}
abstract class EvalIfaceSetNarrowBad implements EvalIfaceSetWide {
    abstract public int $value { set; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_abstract_set, &mut scope, &mut values)
        .expect_err("narrower abstract set property type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let bad_concrete_get = parse_fragment(
        br#"interface EvalIfaceConcreteGetInt {
    public int $value { get; }
}
class EvalIfaceConcreteGetWideBad implements EvalIfaceConcreteGetInt {
    public int|string $value = 4;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_concrete_get, &mut scope, &mut values)
        .expect_err("wider concrete get property type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let bad_inherited_property = parse_fragment(
        br#"interface EvalIfaceInheritedGet {
    public int $value { get; }
}
abstract class EvalIfaceInheritedPropertyBase {
    public string $value = "bad";
}
abstract class EvalIfaceInheritedPropertyChild extends EvalIfaceInheritedPropertyBase implements EvalIfaceInheritedGet {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_inherited_property, &mut scope, &mut values)
        .expect_err("inherited incompatible interface property should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies a get-only hook cannot satisfy a writable eval interface contract.
#[test]
fn execute_program_rejects_get_only_hook_for_interface_set_contract() {
    let program = parse_fragment(
        br#"interface EvalHookSetContract {
    public int $answer { get; set; }
}
class EvalHookGetOnlyContractBox implements EvalHookSetContract {
    public int $answer {
        get => 42;
    }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("get-only hook should fail writable interface contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies readonly properties cannot satisfy writable eval interface contracts.
#[test]
fn execute_program_rejects_readonly_property_for_interface_set_contract() {
    let program = parse_fragment(
        br#"interface EvalReadonlyHookContract {
    public int $id { get; set; }
}
class EvalReadonlyHookContractBox implements EvalReadonlyHookContract {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly property should fail writable interface contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies concrete eval subclasses satisfy abstract property hook contracts.
#[test]
fn execute_program_accepts_abstract_property_hook_contracts() {
    let program = parse_fragment(
        br#"abstract class EvalAbstractHookBase {
    abstract public string $value { get; set; }
}
class EvalAbstractHookBox extends EvalAbstractHookBase {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
$box = new EvalAbstractHookBox();
$box->value = "Ada";
echo $box->value;
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada!");
    assert_eq!(values.get(result), FakeValue::String("Ada!".to_string()));
}

/// Verifies normal mutable properties satisfy abstract get/set hook contracts.
#[test]
fn execute_program_accepts_plain_property_for_abstract_hook_contracts() {
    let program = parse_fragment(
        br#"abstract class EvalPlainAbstractHookBase {
    abstract public string $value { get; set; }
}
class EvalPlainAbstractHookBox extends EvalPlainAbstractHookBase {
    public string $value = "Ada";
}
$box = new EvalPlainAbstractHookBox();
echo $box->value; echo ":";
$box->value = "Grace";
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada:");
    assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
}

/// Verifies concrete eval subclasses must declare inherited abstract properties.
#[test]
fn execute_program_rejects_missing_abstract_property_hook_contract() {
    let program = parse_fragment(
        br#"abstract class EvalMissingAbstractHookBase {
    abstract public string $value { get; }
}
class EvalMissingAbstractHookBox extends EvalMissingAbstractHookBase {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("missing abstract property should fail concrete subclass");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies abstract final eval properties are rejected while parsing.
#[test]
fn parse_fragment_rejects_final_abstract_property_hook_contract() {
    let err = parse_fragment(
        br#"abstract class EvalFinalAbstractHookBase {
    abstract final public string $value { get; }
}"#,
    )
    .expect_err("final abstract property should fail");

    assert_eq!(err, EvalParseError::UnsupportedConstruct);
}

/// Verifies readonly properties cannot satisfy abstract writable hook contracts.
#[test]
fn execute_program_rejects_readonly_property_for_abstract_set_contract() {
    let program = parse_fragment(
        br#"abstract class EvalReadonlyAbstractHookBase {
    abstract public int $id { get; set; }
}
class EvalReadonlyAbstractHookBox extends EvalReadonlyAbstractHookBase {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("readonly property should fail abstract writable contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies abstract trait property hook contracts are enforced after trait expansion.
#[test]
fn execute_program_enforces_trait_abstract_property_hook_contracts() {
    let program = parse_fragment(
        br#"trait EvalTraitNeedsName {
    abstract protected string $name { get; }
    public function label() { return $this->name; }
}
class EvalTraitNameBox {
    use EvalTraitNeedsName;
    protected string $name = "Ada";
}
$box = new EvalTraitNameBox();
echo $box->label();
return $box->label();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Ada");
    assert_eq!(values.get(result), FakeValue::String("Ada".to_string()));
}
