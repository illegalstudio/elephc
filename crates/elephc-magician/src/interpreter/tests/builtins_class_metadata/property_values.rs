//! Purpose:
//! Interpreter tests for ReflectionProperty value access and parameter construction.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Static, instance, dynamic, hooked, raw, and initialized-state paths are covered.

use super::super::super::*;
use super::super::support::*;

/// Verifies ReflectionProperty can read and write eval instance and static property values.
#[test]
fn execute_program_reflection_property_gets_and_sets_eval_values() {
    let program = parse_fragment(
        br#"class EvalReflectValueBase {
    private $secret = "base";
    public static $count = 1;
}
class EvalReflectValueChild extends EvalReflectValueBase {
    protected $name = "Ada";
}
class EvalReflectValueHook {
    public $raw = 2;
    public $doubled {
        get => $this->raw * 2;
        set { $this->raw = $value + 1; }
    }
    public $backed {
        get { return $this->backed * 2; }
        set { $this->backed = $value; }
    }
    public $virtual {
        get => $this->raw + 100;
    }
    public function __construct() {
        $this->backed = 2;
    }
}
$child = new EvalReflectValueChild();
$secret = new ReflectionProperty("EvalReflectValueBase", "secret");
echo $secret->getValue($child); echo ":";
$secret->setValue($child, "changed");
echo $secret->getValue(object: $child); echo ":";
$name = new ReflectionProperty("EvalReflectValueChild", "name");
echo $name->getValue($child); echo ":";
$name->setValue(objectOrValue: $child, value: "Grace");
echo $name->getValue($child); echo ":";
$count = new ReflectionProperty("EvalReflectValueBase", "count");
echo $count->getValue(); echo ":";
$count->setValue(5);
echo EvalReflectValueChild::$count; echo ":";
$count->setValue(null, 6);
echo $count->getValue($child); echo ":";
$hook = new EvalReflectValueHook();
$doubled = new ReflectionProperty("EvalReflectValueHook", "doubled");
echo $doubled->getValue($hook); echo ":";
$doubled->setValue($hook, 4);
echo $hook->raw; echo ":";
echo $doubled->getValue($hook); echo ":";
$backed = new ReflectionProperty("EvalReflectValueHook", "backed");
echo $backed->getRawValue($hook); echo ":";
echo $backed->getValue($hook); echo ":";
$backed->setValue($hook, 4);
echo $backed->getRawValue(object: $hook); echo ":";
echo $backed->getValue($hook); echo ":";
$backed->setRawValue(object: $hook, value: 7);
echo $backed->getRawValue($hook); echo ":";
echo $backed->getValue($hook); echo ":";
echo $backed->isLazy($hook) ? "L" : "l"; echo ":";
$backed->skipLazyInitialization(object: $hook);
$backed->setRawValueWithoutLazyInitialization(object: $hook, value: 8);
echo $backed->getRawValue($hook); echo ":";
echo $backed->getValue($hook); echo ":";
echo $backed->getModifiers(); echo ":";
echo $backed->isVirtual() ? "V" : "b"; echo ":";
echo (new ReflectionProperty("EvalReflectValueHook", "virtual"))->isVirtual() ? "V" : "b"; echo ":";
echo (new ReflectionProperty("EvalReflectValueHook", "virtual"))->getModifiers();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "base:changed:Ada:Grace:1:5:6:4:5:10:2:4:4:8:7:14:l:8:16:1:b:V:513"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionProperty raw APIs reject virtual eval property hooks.
#[test]
fn execute_program_reflection_property_rejects_virtual_raw_value() {
    let program = parse_fragment(
        br#"class EvalReflectVirtualRawHook {
    public $raw = 2;
    public $virtual {
        get => $this->raw * 2;
    }
}
$object = new EvalReflectVirtualRawHook();
$property = new ReflectionProperty("EvalReflectVirtualRawHook", "virtual");
$property->getRawValue($object);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("virtual raw property read should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies ReflectionProperty reports eval instance and static initialization state.
#[test]
fn execute_program_reflection_property_reports_initialized_state() {
    let program = parse_fragment(
        br#"class EvalReflectInitializedTarget {
    public int $typed;
    public ?int $nullable;
    public $plain;
    public static int $staticTyped;
    public static $staticPlain;
    public $virtual {
        get => 42;
    }
}
$object = new EvalReflectInitializedTarget();
$typed = new ReflectionProperty("EvalReflectInitializedTarget", "typed");
$nullable = new ReflectionProperty("EvalReflectInitializedTarget", "nullable");
$plain = new ReflectionProperty("EvalReflectInitializedTarget", "plain");
$staticTyped = new ReflectionProperty("EvalReflectInitializedTarget", "staticTyped");
$staticPlain = new ReflectionProperty("EvalReflectInitializedTarget", "staticPlain");
$virtual = new ReflectionProperty("EvalReflectInitializedTarget", "virtual");
echo $typed->isInitialized($object) ? "T" : "t"; echo ":";
echo $plain->isInitialized(object: $object) ? "P" : "p"; echo ":";
echo $staticTyped->isInitialized() ? "S" : "s"; echo ":";
echo $staticPlain->isInitialized() ? "N" : "n"; echo ":";
EvalReflectInitializedTarget::$staticTyped = 3;
echo $staticTyped->isInitialized() ? "S" : "s"; echo ":";
$object->typed = 5;
echo $typed->isInitialized($object) ? "T" : "t"; echo ":";
unset($object->typed);
echo $typed->isInitialized($object) ? "T" : "t"; echo ":";
$typed->setRawValue(object: $object, value: 9);
echo $typed->isInitialized($object) ? "T" : "t"; echo ":";
echo $nullable->isInitialized($object) ? "Y" : "y"; echo ":";
$nullable->setValue($object, null);
echo $nullable->isInitialized($object) ? "Y" : "y"; echo ":";
echo $virtual->isInitialized($object) ? "V" : "v";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "t:P:s:N:S:T:t:T:y:Y:V");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionProperty materializes and operates on public dynamic properties.
#[test]
fn execute_program_reflection_property_supports_dynamic_properties() {
    let program = parse_fragment(
        br#"class EvalReflectDynamicBase {}
class EvalReflectDynamicChild extends EvalReflectDynamicBase {}
$object = new EvalReflectDynamicBase();
$object->dynamic = "first";
$child = new EvalReflectDynamicChild();
$child->dynamic = "child";
$empty = new EvalReflectDynamicChild();
$property = new ReflectionProperty($object, "dynamic");
echo $property->getName(); echo ":";
echo $property->isDynamic() ? "D" : "d"; echo ":";
echo $property->isDefault() ? "Y" : "N"; echo ":";
echo $property->getModifiers(); echo ":";
echo is_null($property->getType()) ? "T" : "t"; echo ":";
echo is_null($property->getSettableType()) ? "S" : "s"; echo ":";
echo $property->hasDefaultValue() ? "H" : "h"; echo ":";
echo is_null($property->getDefaultValue()) ? "V" : "v"; echo ":";
echo $property->isLazy($object) ? "L" : "l"; echo ":";
echo $property->isInitialized($object) ? "I" : "i"; echo ":";
echo $property->getValue($object); echo ":";
echo $property->getValue($child); echo ":";
echo $property->isInitialized($empty) ? "E" : "e"; echo ":";
echo is_null($property->getValue($empty)) ? "null" : "bad"; echo ":";
$property->setValue($empty, "filled");
echo $property->getValue($empty); echo ":";
$property->setRawValue($object, "raw");
echo $property->getRawValue($object); echo ":";
echo str_replace("\n", "\\n", $property->__toString());
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "dynamic:D:N:1:T:S:h:V:l:I:first:child:e:null:filled:raw:Property [ <dynamic> public $dynamic ]\\n"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionProperty exposes eval property hook metadata and methods.
#[test]
fn execute_program_reflection_property_gets_eval_hook_metadata() {
    let program = parse_fragment(
        br#"class EvalReflectHookedProperty {
    public int $raw = 2;
    public int $doubled {
        get { return $this->raw * 2; }
        set { $this->raw = $value; }
    }
    public int $readonlyHook {
        get => $this->raw + 1;
    }
    public int $plain = 5;
}
abstract class EvalReflectAbstractHookProperty {
    abstract public int $contract { get; set; }
}
interface EvalReflectInterfaceHookProperty {
    public int $iface { get; }
}
$hooked = new ReflectionProperty("EvalReflectHookedProperty", "doubled");
$plain = new ReflectionProperty("EvalReflectHookedProperty", "plain");
$readonly = new ReflectionProperty("EvalReflectHookedProperty", "readonlyHook");
$abstract = new ReflectionProperty("EvalReflectAbstractHookProperty", "contract");
$iface = new ReflectionProperty("EvalReflectInterfaceHookProperty", "iface");
$getCase = PropertyHookType::Get;
$setCase = PropertyHookType::Set;
echo $getCase->name; echo ":"; echo $getCase->value; echo ":";
$caseList = PropertyHookType::cases();
echo count($caseList); echo ":"; echo $caseList[0]->name; echo ":"; echo $caseList[1]->value; echo ":";
echo PropertyHookType::from("set")->name; echo ":";
echo PropertyHookType::tryFrom("missing") === null ? "T" : "t"; echo ":";
echo $hooked->hasHooks() ? "H" : "h"; echo ":";
echo $hooked->hasHook($getCase) ? "G" : "g"; echo ":";
echo $hooked->hasHook(type: $setCase) ? "S" : "s"; echo ":";
$hooks = $hooked->getHooks();
echo count($hooks); echo ":"; echo $hooks["get"]->getName(); echo ":"; echo $hooks["set"]->getName(); echo ":";
$get = $hooked->getHook($getCase);
$set = $hooked->getHook(type: $setCase);
echo $get->getDeclaringClass()->getName(); echo ":"; echo $get->getNumberOfParameters(); echo ":";
echo $set->getNumberOfParameters(); echo ":"; echo $set->getParameters()[0]->getName(); echo ":";
$box = new EvalReflectHookedProperty();
echo $get->invoke($box); echo ":";
$set->invoke($box, 7);
echo $box->raw; echo ":";
echo $readonly->hasHook($getCase) ? "R" : "r"; echo ":";
echo $readonly->hasHook($setCase) ? "w" : "W"; echo ":";
echo $readonly->getHook($setCase) === null ? "N" : "n"; echo ":";
echo $plain->hasHooks() ? "bad" : "plain"; echo ":";
echo count($plain->getHooks()); echo ":";
$abstractHooks = $abstract->getHooks();
echo count($abstractHooks); echo ":";
echo $abstract->hasHook($getCase) ? "AG" : "ag"; echo ":";
echo $abstract->hasHook($setCase) ? "AS" : "as"; echo ":";
echo $abstractHooks["get"]->getName(); echo ":"; echo $abstractHooks["get"]->isAbstract() ? "A" : "a"; echo ":";
echo $abstractHooks["set"]->getName(); echo ":"; echo $abstractHooks["set"]->isAbstract() ? "A" : "a"; echo ":";
$ifaceHook = $iface->getHook($getCase);
echo count($iface->getHooks()); echo ":";
echo $iface->hasHook($getCase) ? "IG" : "ig"; echo ":";
echo $iface->hasHook($setCase) ? "bad" : "is"; echo ":";
echo $ifaceHook->isAbstract() ? "IA" : "ia";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Get:get:2:Get:set:Set:T:H:G:S:2:$doubled::get:$doubled::set:EvalReflectHookedProperty:0:1:value:4:7:R:W:N:plain:0:2:AG:AS:$contract::get:A:$contract::set:A:1:IG:is:IA"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass exposes and mutates eval static property values.
#[test]
fn execute_program_reflection_class_static_property_values() {
    let program = parse_fragment(
        br#"class EvalReflectStaticBase {
    public static $base = "b";
    protected static $prot = "p";
    private static $shadow = "base-hidden";
    public $instance = "i";
}
class EvalReflectStaticChild extends EvalReflectStaticBase {
    public static $child = "c";
    private static $shadow = "child-hidden";
    public static int $count = 1;
}
EvalReflectStaticChild::$child = "mut";
$ref = new ReflectionClass("EvalReflectStaticChild");
$statics = $ref->getStaticProperties();
echo count($statics); echo ":";
echo $statics["child"]; echo ":";
echo $statics["base"]; echo ":";
echo $statics["prot"]; echo ":";
echo $statics["shadow"]; echo ":";
echo $ref->getStaticPropertyValue("count"); echo ":";
$ref->setStaticPropertyValue("shadow", "changed");
echo $ref->getStaticPropertyValue("shadow"); echo ":";
$ref->setStaticPropertyValue(name: "count", value: 5);
echo EvalReflectStaticChild::$count; echo ":";
echo $ref->getStaticPropertyValue("instance", "fallback"); echo ":";
echo $ref->getStaticPropertyValue("missing", "fallback"); echo ":";
try {
    $ref->getStaticPropertyValue("missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo "E";
}
echo ":";
try {
    $ref->setStaticPropertyValue("instance", "bad");
    echo "bad";
} catch (ReflectionException $e) {
    echo "S";
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "5:mut:b:p:child-hidden:1:changed:5:fallback:fallback:E:S"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval ReflectionParameter exposes declaring class metadata.
#[test]
fn execute_program_reflects_eval_parameter_declaring_class() {
    let program = parse_fragment(
        br#"class EvalDeclaringParamBase {
    public function inherited($base) {}
}
class EvalDeclaringParamChild extends EvalDeclaringParamBase {
    public function own($child) {}
}
$inherited = (new ReflectionMethod("EvalDeclaringParamChild", "inherited"))->getParameters()[0];
echo $inherited->getDeclaringClass()->getName(); echo ":";
echo $inherited->getDeclaringFunction()->getName(); echo ":";
echo $inherited->getDeclaringFunction()->getDeclaringClass()->getName(); echo ":";
$listed = (new ReflectionMethod("EvalDeclaringParamChild", "own"))->getParameters()[0];
echo $listed->getDeclaringClass()->getName(); echo ":";
echo $listed->getDeclaringFunction()->getName();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "EvalDeclaringParamBase:inherited:EvalDeclaringParamBase:EvalDeclaringParamChild:own"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies direct ReflectionParameter construction accepts runtime object method targets.
#[test]
fn execute_program_reflection_parameter_accepts_object_expression_target() {
    let program = parse_fragment(
        br#"class EvalDirectParamObjectTarget {
    public function run(int $id, ?string $name = null) {}
}
$param = new ReflectionParameter([new EvalDirectParamObjectTarget(), "run"], "name");
echo $param->getName(); echo ":";
echo $param->getPosition(); echo ":";
echo $param->getDeclaringClass()->getName(); echo ":";
echo $param->getDeclaringFunction()->getName(); echo ":";
echo $param->isOptional() ? "O" : "R"; echo ":";
echo $param->getType()->getName(); echo ":";
echo $param->allowsNull() ? "N" : "n";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "name:1:EvalDirectParamObjectTarget:run:O:string:N"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionParameter construction throws catchable PHP reflection errors.
#[test]
fn execute_program_reflection_parameter_constructor_throws_reflection_exceptions() {
    let program = parse_fragment(
        br#"function eval_reflect_param_error_function($known) {}
class EvalReflectParamErrorTarget {
    public function run($known) {}
}
try {
    new ReflectionParameter("eval_reflect_param_error_function", "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    new ReflectionParameter(["EvalReflectParamErrorTarget", "run"], "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionParameter(["EvalReflectParamErrorTarget", "run"], 3);
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionParameter(["EvalReflectParamErrorTarget", "missing"], "known");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionParameter(["EvalReflectParamErrorTarget", "run"], -1);
    echo "bad";
} catch (ValueError $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
echo (new ReflectionParameter(["EvalReflectParamErrorTarget", "run"], "known"))->getName();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);
    assert!(
        result.is_ok(),
        "execute eval ir failed after output {:?}",
        values.output
    );

    assert_eq!(
        values.output,
        "ReflectionException:The parameter specified by its name could not be found|The parameter specified by its name could not be found|The parameter specified by its offset could not be found|Method EvalReflectParamErrorTarget::missing() does not exist|ValueError:ReflectionParameter::__construct(): Argument #2 ($param) must be greater than or equal to 0|known"
    );
    assert_eq!(values.get(result.expect("execute eval ir")), FakeValue::Bool(true));
}
