//! Purpose:
//! Interpreter tests for ReflectionClass member lists, instantiation, and invocation.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Object allocation and reflected method calls retain PHP visibility checks.

use super::super::super::*;
use super::super::support::*;

/// Verifies ReflectionClass::getMethods preserves eval method parameter metadata.
#[test]
fn execute_program_reflection_class_lists_eval_method_parameters() {
    let program = parse_fragment(
        br#"class EvalReflectListedParamTarget {
    public function first($left) {}
    public function second($right, $tail) {}
}
$methods = (new ReflectionClass("EvalReflectListedParamTarget"))->getMethods();
foreach ($methods as $method) {
    $params = $method->getParameters();
    echo $method->getName(); echo ":";
    echo $method->getNumberOfParameters(); echo "/";
    echo $method->getNumberOfRequiredParameters();
    if (count($params) > 0) {
        echo ":"; echo $params[0]->getName(); echo ":"; echo $params[0]->getPosition();
    }
    echo "|";
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "first:1/1:left:0|second:2/2:right:0|");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass getMethods/getProperties return eval member objects.
#[test]
fn execute_program_reflection_class_lists_eval_member_objects() {
    let program = parse_fragment(
        br#"#[Attribute]
class EvalListMarker {}
class EvalReflectListTarget {
    #[EvalListMarker]
    public function first() {}
    private static function helper() {}
    #[EvalListMarker]
    protected $visible;
    private static $token;
}
$ref = new ReflectionClass("EvalReflectListTarget");
$methods = $ref->getMethods();
$properties = $ref->getProperties();
$staticMethods = $ref->getMethods(ReflectionMethod::IS_STATIC);
$privateMethods = $ref->getMethods(filter: ReflectionMethod::IS_PRIVATE);
$noMethods = $ref->getMethods(0);
$nullMethods = $ref->getMethods(null);
$staticProperties = $ref->getProperties(ReflectionProperty::IS_STATIC);
$protectedProperties = $ref->getProperties(filter: ReflectionProperty::IS_PROTECTED);
$noProperties = $ref->getProperties(0);
echo count($methods); echo ":"; echo count($properties); echo ":";
echo ReflectionMethod::IS_STATIC; echo ":"; echo ReflectionMethod::IS_PRIVATE; echo ":";
$direct = new ReflectionMethod("EvalReflectListTarget", "helper");
echo "D"; echo $direct->getModifiers(); echo ":";
foreach ($methods as $method) {
    if ($method->getName() === "first") {
        echo "F"; echo count($method->getAttributes());
        echo "M"; echo $method->getModifiers();
    }
    if ($method->getName() === "helper") {
        echo $method->isStatic() ? "S" : "s";
        echo $method->isPrivate() ? "R" : "r";
        echo "M"; echo $method->getModifiers();
    }
}
echo ":";
foreach ($properties as $property) {
    if ($property->getName() === "visible") {
        echo "V"; echo count($property->getAttributes());
        echo $property->isProtected() ? "P" : "p";
        echo "M"; echo $property->getModifiers();
    }
    if ($property->getName() === "token") {
        echo $property->isStatic() ? "T" : "t";
        echo $property->isPrivate() ? "R" : "r";
        echo "M"; echo $property->getModifiers();
    }
}
echo ":";
echo count($staticMethods); echo $staticMethods[0]->getName(); echo ":";
echo count($privateMethods); echo $privateMethods[0]->getName(); echo ":";
echo count($noMethods); echo ":"; echo count($nullMethods); echo ":";
echo count($staticProperties); echo $staticProperties[0]->getName(); echo ":";
echo count($protectedProperties); echo $protectedProperties[0]->getName(); echo ":";
echo count($noProperties);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "2:2:16:4:D20:F1M1SRM20:V1PM2TRM20:1helper:1helper:0:2:1token:1visible:0"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass getMethod/getProperty return eval member objects.
#[test]
fn execute_program_reflection_class_gets_eval_member_objects() {
    let program = parse_fragment(
        br#"class EvalReflectLookupTarget {
    public function first() {}
    private static function helper() {}
    protected $visible;
    private static $token;
}
$ref = new ReflectionClass("EvalReflectLookupTarget");
$method = $ref->getMethod("FIRST");
echo $method->getName(); echo ":";
echo $method->isPublic() ? "U" : "u"; echo ":";
$helper = $ref->getMethod("helper");
echo $helper->isPrivate() ? "P" : "p";
echo $helper->isStatic() ? "S" : "s"; echo ":";
$property = $ref->getProperty("visible");
echo $property->getName(); echo ":";
echo $property->isProtected() ? "R" : "r";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "first:U:PS:visible:R");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::getParentClass returns eval parent metadata or false.
#[test]
fn execute_program_reflection_class_get_parent_class() {
    let program = parse_fragment(
        br#"class EvalReflectParentBase {}
class EvalReflectParentChild extends EvalReflectParentBase {}
$parent = (new ReflectionClass("EvalReflectParentChild"))->getParentClass();
echo $parent->getName();
echo ":";
$root = (new ReflectionClass("EvalReflectParentBase"))->getParentClass();
if ($root === false) {
    echo "false";
} else {
    echo "bad";
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "EvalReflectParentBase:false");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::newInstance constructs eval-declared classes.
#[test]
fn execute_program_reflection_class_new_instance_constructs_eval_class() {
    let program = parse_fragment(
        br#"class EvalReflectNewTarget {
    public $label;
    public function __construct($left, $right) {
        $this->label = $left . $right;
    }
    public function label() {
        return $this->label;
    }
}
$ref = new ReflectionClass("EvalReflectNewTarget");
$first = $ref->newInstance("I", "J");
echo $first->label(); echo ":";
$second = $ref->newInstance(...["K", "L"]);
echo $second->label(); echo ":";
$third = $ref->newInstanceArgs(["right" => "N", "left" => "M"]);
echo $third->label(); echo ":";
$fourth = $ref->newInstanceArgs(["O", "P"]);
echo $fourth->label();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "IJ:KL:MN:OP");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::newInstance throws for non-public eval constructors.
#[test]
fn execute_program_reflection_class_new_instance_rejects_non_public_eval_constructors() {
    let program = parse_fragment(
        br#"class EvalReflectNewPrivateCtor {
    private function __construct() {}
}
class EvalReflectNewProtectedCtor {
    protected function __construct() {}
}
try {
    (new ReflectionClass("EvalReflectNewPrivateCtor"))->newInstance();
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    (new ReflectionClass("EvalReflectNewProtectedCtor"))->newInstance();
    echo "bad";
} catch (ReflectionException $e) {
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
        "ReflectionException:Access to non-public constructor of class EvalReflectNewPrivateCtor|ReflectionException:Access to non-public constructor of class EvalReflectNewProtectedCtor"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass instantiation throws Error for eval non-instantiable class-likes.
#[test]
fn execute_program_reflection_class_new_instance_rejects_eval_non_instantiable_class_likes() {
    let program = parse_fragment(
        br#"abstract class EvalReflectNewAbstract {}
interface EvalReflectNewIface {}
trait EvalReflectNewTrait {}
enum EvalReflectNewEnum { case Ready; }
function eval_reflect_new_error($class, $without) {
    try {
        $ref = new ReflectionClass($class);
        if ($without) {
            $ref->newInstanceWithoutConstructor();
        } else {
            $ref->newInstance();
        }
        echo "bad";
    } catch (Error $e) {
        echo get_class($e) . ":" . $e->getMessage();
    }
}
eval_reflect_new_error("EvalReflectNewAbstract", false); echo "|";
eval_reflect_new_error("EvalReflectNewAbstract", true); echo "|";
eval_reflect_new_error("EvalReflectNewIface", false); echo "|";
eval_reflect_new_error("EvalReflectNewIface", true); echo "|";
eval_reflect_new_error("EvalReflectNewTrait", false); echo "|";
eval_reflect_new_error("EvalReflectNewTrait", true); echo "|";
eval_reflect_new_error("EvalReflectNewEnum", false); echo "|";
eval_reflect_new_error("EvalReflectNewEnum", true);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Cannot instantiate abstract class EvalReflectNewAbstract|Error:Cannot instantiate abstract class EvalReflectNewAbstract|Error:Cannot instantiate interface EvalReflectNewIface|Error:Cannot instantiate interface EvalReflectNewIface|Error:Cannot instantiate trait EvalReflectNewTrait|Error:Cannot instantiate trait EvalReflectNewTrait|Error:Cannot instantiate enum EvalReflectNewEnum|Error:Cannot instantiate enum EvalReflectNewEnum"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod::invoke dispatches eval-declared methods.
#[test]
fn execute_program_reflection_method_invoke_calls_eval_method() {
    let program = parse_fragment(
        br#"class EvalReflectInvokeBase {
    private function hidden($label = "H") {
        return "hidden:" . $label;
    }
    public function who() {
        return static::class;
    }
    public static function make($left, $right = "S") {
        return static::class . ":" . $left . $right;
    }
}
class EvalReflectInvokeChild extends EvalReflectInvokeBase {
    public function join($a, $b = "B") {
        return $a . $b;
    }
    public function mutate(&$value) {
        $value = $value . "!";
        return $value;
    }
}
$object = new EvalReflectInvokeChild();
$hidden = new ReflectionMethod("EvalReflectInvokeBase", "hidden");
echo $hidden->invoke($object, "X"); echo ":";
$who = (new ReflectionClass("EvalReflectInvokeChild"))->getMethod("who");
echo $who->invoke($object); echo ":";
$static = new ReflectionMethod("EvalReflectInvokeBase", "make");
echo $static->invoke(null, right: "Y", left: "X"); echo ":";
echo $static->invoke($object, "A"); echo ":";
$join = null;
foreach ((new ReflectionClass("EvalReflectInvokeChild"))->getMethods() as $method) {
    if ($method->getName() === "join") {
        $join = $method;
    }
}
$value = "Q";
$mutate = new ReflectionMethod("EvalReflectInvokeChild", "mutate");
echo $join->invokeArgs($object, ["b" => "2", "a" => "1"]); echo ":";
echo $mutate->invoke($object, $value); echo ":"; echo $value;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "hidden:X:EvalReflectInvokeChild:EvalReflectInvokeBase:XY:EvalReflectInvokeBase:AS:12:Q!:Q"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod::invoke throws for incompatible eval receivers.
#[test]
fn execute_program_reflection_method_invoke_rejects_wrong_object() {
    let program = parse_fragment(
        br#"class EvalReflectInvokeOwner {
    public function run() {
        return "owner";
    }
}
class EvalReflectInvokeOther {}
try {
    (new ReflectionMethod("EvalReflectInvokeOwner", "run"))->invoke(new EvalReflectInvokeOther());
    echo "bad";
} catch (ReflectionException $e) {
    echo "caught";
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "caught");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod/Property::setAccessible are PHP-compatible no-ops.
#[test]
fn execute_program_reflection_set_accessible_is_noop() {
    let program = parse_fragment(
        br#"class EvalReflectAccessTarget {
    private $secret = "s";
    private function hidden() {
        return $this->secret;
    }
}
$object = new EvalReflectAccessTarget();
$method = new ReflectionMethod("EvalReflectAccessTarget", "hidden");
echo is_null($method->setAccessible(false)) ? "M" : "m"; echo ":";
echo $method->invoke($object); echo ":";
$property = new ReflectionProperty("EvalReflectAccessTarget", "secret");
echo is_null($property->setAccessible(accessible: true)) ? "P" : "p"; echo ":";
echo $property->getValue($object);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "M:s:P:s");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::newInstanceWithoutConstructor skips eval constructors.
#[test]
fn execute_program_reflection_class_new_instance_without_constructor_allocates_eval_class() {
    let program = parse_fragment(
        br#"class EvalReflectNoCtorTarget {
    public $label = "default";
    private $secret = "hidden";
    public function __construct() {
        $this->label = "ctor";
    }
    public function label() {
        return $this->label;
    }
    public function secret() {
        return $this->secret;
    }
}
$ref = new ReflectionClass("EvalReflectNoCtorTarget");
$without = $ref->newInstanceWithoutConstructor();
echo $without->label(); echo ":";
echo $without->secret(); echo ":";
$with = $ref->newInstance();
echo $with->label();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "default:hidden:ctor");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
