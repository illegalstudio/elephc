//! Purpose:
//! Interpreter tests for reflected callable parameters, types, and string formatting.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Method, parameter, property, and composite type metadata stay PHP-compatible.

use super::super::super::*;
use super::super::support::*;

/// Verifies ReflectionMethod exposes eval method parameter objects with names and positions.
#[test]
fn execute_program_reflects_eval_method_parameters() {
    let program = parse_fragment(
br##"interface EvalReflectLeft {}
interface EvalReflectRight {}
class EvalReflectParamTarget {
    public function run(#[EvalParamTag("first")] int &$first, int|string $union, #[EvalParamTag("both")] EvalReflectLeft&EvalReflectRight $both, ?array $items = null, ?callable $callback = null, \App\Name|null $second = null, &...$rest) {}
}
$method = new ReflectionMethod("EvalReflectParamTarget", "run");
echo $method->getNumberOfParameters(); echo "/";
echo $method->getNumberOfRequiredParameters(); echo ":";
$params = $method->getParameters();
foreach ($params as $param) {
    echo $param->getName(); echo "#"; echo $param->getPosition();
    echo $param->isOptional() ? "O" : "r";
    echo $param->isVariadic() ? "V" : "v";
    echo $param->isPassedByReference() ? "R" : "b";
    echo $param->canBePassedByValue() ? "Y" : "N";
    echo $param->hasType() ? "T" : "t";
    echo $param->allowsNull() ? "N" : "n";
    echo $param->isArray() ? "A" : "a";
    echo $param->isCallable() ? "C" : "c";
    $type = $param->getType();
    if ($param->getName() == "union") {
        echo ":union";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":"; echo $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } elseif ($param->getName() == "both") {
        echo ":intersection";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":"; echo $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } elseif ($type) {
        echo ":"; echo $type->getName();
        echo $type->allowsNull() ? "?" : "!";
        echo $type->isBuiltin() ? "B" : "C";
    } else {
        echo ":null";
    }
    $attrs = $param->getAttributes();
    echo ":A"; echo count($attrs);
    if (count($attrs) > 0) {
        echo ":"; echo $attrs[0]->getName();
        echo ":"; echo $attrs[0]->getArguments()[0];
    }
    echo $param->isDefaultValueAvailable() ? ":D" : ":d";
    if ($param->isDefaultValueAvailable()) {
        echo "=";
        echo $param->getDefaultValue() === null ? "null" : $param->getDefaultValue();
    }
    echo "|";
}
return true;"##,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "7/3:first#0rvRNTnac:int!B:A1:EvalParamTag:first:d|union#1rvbYTnac:union!:intB:stringB:A0:d|both#2rvbYTnac:intersection!:EvalReflectLeftC:EvalReflectRightC:A1:EvalParamTag:both:d|items#3OvbYTNAc:array?B:A0:D=null|callback#4OvbYTNaC:callable?B:A0:D=null|second#5OvbYTNac:App\\Name?C:A0:D=null|rest#6OVRNtNac:null:A0:d|"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionType objects stringify retained eval parameter metadata.
#[test]
fn execute_program_reflection_type_to_string() {
    let program = parse_fragment(
br##"class EvalReflectTypeStringDep {}
interface EvalReflectTypeStringLeft {}
interface EvalReflectTypeStringRight {}
class EvalReflectTypeStringTarget {
    public function run(?EvalReflectTypeStringDep $dep, int|string|null $union, EvalReflectTypeStringLeft&EvalReflectTypeStringRight $both, mixed $mixed, ?array $items) {}
}
$params = (new ReflectionMethod("EvalReflectTypeStringTarget", "run"))->getParameters();
foreach ($params as $param) {
    $type = $param->getType();
    echo $param->getName(); echo ":";
    echo $type->__toString(); echo "|";
}
$unionType = $params[1]->getType();
echo "cast:" . (string)$unionType . "|";
echo "concat:" . $unionType . "|";
echo "echo:";
echo $unionType;
return true;"##,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "dep:?EvalReflectTypeStringDep|union:int|string|null|both:EvalReflectTypeStringLeft&EvalReflectTypeStringRight|mixed:mixed|items:?array|cast:int|string|null|concat:int|string|null|echo:int|string|null"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionParameter formats retained eval parameter metadata through `__toString()`.
#[test]
fn execute_program_reflection_parameter_to_string() {
    let program = parse_fragment(
        br#"class EvalReflectParameterStringTarget {
    const LABEL = "L";
    public function run(string $name, int $count = 3, $label = self::LABEL, &...$items) {}
}
$params = (new ReflectionMethod("EvalReflectParameterStringTarget", "run"))->getParameters();
foreach ($params as $param) {
    echo $param->__toString();
    echo "|";
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Parameter #0 [ <required> string $name ]|Parameter #1 [ <optional> int $count = 3 ]|Parameter #2 [ <optional> $label = self::LABEL ]|Parameter #3 [ <optional> &...$items ]|"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod exposes eval-declared return type metadata.
#[test]
fn execute_program_reflection_method_reports_return_type_metadata() {
    let program = parse_fragment(
        br#"interface EvalReflectReturnIface {
    public function read(): string;
}
class EvalReflectReturnTarget implements EvalReflectReturnIface {
    public function read(): string { return "ok"; }
    public function selfReturn(): static { return $this; }
    public function done(): void {}
}
$iface = new ReflectionMethod("EvalReflectReturnIface", "read");
$ifaceType = $iface->getReturnType();
echo $iface->hasReturnType() ? "I" : "i"; echo ":";
echo $ifaceType->getName(); echo ":";
echo $ifaceType->isBuiltin() ? "B" : "b"; echo ":";
$self = (new ReflectionMethod("EvalReflectReturnTarget", "selfReturn"))->getReturnType();
echo $self->getName(); echo ":";
echo $self->isBuiltin() ? "B" : "b"; echo ":";
$void = (new ReflectionMethod("EvalReflectReturnTarget", "done"))->getReturnType();
echo $void->getName(); echo ":";
echo $void->allowsNull() ? "N" : "n"; echo ":";
echo $void->isBuiltin() ? "B" : "b";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "I:string:B:static:b:void:n:B");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod formats retained eval method metadata through `__toString()`.
#[test]
fn execute_program_reflection_method_to_string() {
    let program = parse_fragment(
        br#"class EvalReflectMethodStringTarget {
    final public static function run(?int $id, string $label = "ok"): ?string {
        return $label;
    }
}
$ref = new ReflectionMethod("EvalReflectMethodStringTarget", "run");
echo str_replace("\n", "|", $ref->__toString());
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Method [ <user> final static public method run ] {|  - Parameters [2] {|    Parameter #0 [ <required> ?int $id ]|    Parameter #1 [ <optional> string $label = 'ok' ]|  }|  - Return [ ?string ]|}|"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionParameter reports eval constructor-promotion metadata.
#[test]
fn execute_program_reflection_parameter_reports_eval_promoted_metadata() {
    let program = parse_fragment(
        br#"class EvalReflectPromotedParamTarget {
    public function __construct(public int $id, string $name = "Ada") {}
    public function run(int $id) {}
}
$ctorParams = (new ReflectionMethod("EvalReflectPromotedParamTarget", "__construct"))->getParameters();
$runParams = (new ReflectionMethod("EvalReflectPromotedParamTarget", "run"))->getParameters();
echo $ctorParams[0]->isPromoted() ? "I" : "i";
echo $ctorParams[1]->isPromoted() ? "N" : "n";
echo $runParams[0]->isPromoted() ? "R" : "r";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Inr");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionProperty exposes eval property get/set type metadata.
#[test]
fn execute_program_reflection_property_get_type_metadata() {
    let program = parse_fragment(
        br##"class EvalReflectPropertyTypeDep {}
class EvalReflectPropertyTypeTarget {
    public int $id;
    public ?string $name;
    public EvalReflectPropertyTypeDep $dep;
    public $plain;
    public int|string $union;
}
$properties = (new ReflectionClass("EvalReflectPropertyTypeTarget"))->getProperties();
foreach ($properties as $property) {
    echo $property->getName(); echo ":";
    echo $property->hasType() ? "T:" : "t:";
    $type = $property->getType();
    if ($property->getName() == "union") {
        echo "union";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":"; echo $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } elseif ($type) {
        echo $type->getName();
        echo $type->allowsNull() ? "?" : "!";
        echo $type->isBuiltin() ? "B" : "C";
    } else {
        echo "null";
    }
    echo "|";
}
$direct = new ReflectionProperty("EvalReflectPropertyTypeTarget", "dep");
$directType = $direct->getType();
echo "direct:"; echo $direct->hasType() ? "T:" : "t:";
echo $directType->getName();
$directSettableType = $direct->getSettableType();
echo ":set:"; echo $directSettableType->getName();
$plain = new ReflectionProperty("EvalReflectPropertyTypeTarget", "plain");
echo ":plainSet:"; echo $plain->getSettableType() === null ? "N" : "n";
$directUnion = new ReflectionProperty("EvalReflectPropertyTypeTarget", "union");
echo ":unionSet:"; echo count($directUnion->getSettableType()->getTypes());
return true;"##,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "id:T:int!B|name:T:string?B|dep:T:EvalReflectPropertyTypeDep!C|plain:t:null|union:T:union!:intB:stringB|direct:T:EvalReflectPropertyTypeDep:set:EvalReflectPropertyTypeDep:plainSet:N:unionSet:2"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionProperty retains explicit set-hook parameter metadata as the settable type.
#[test]
fn execute_program_reflection_property_get_settable_type_uses_set_hook_parameter() {
    let program = parse_fragment(
        br##"class EvalReflectSettableTypeTarget {
    public string $value {
        get => $this->value;
        set(int|string $raw) => (string) $raw;
    }
}
$property = new ReflectionProperty("EvalReflectSettableTypeTarget", "value");
$type = $property->getType();
$settable = $property->getSettableType();
echo $type->getName(); echo ":";
echo count($settable->getTypes());
foreach ($settable->getTypes() as $memberType) {
    echo ":"; echo $memberType->getName();
    echo $memberType->isBuiltin() ? "B" : "C";
}
$setHook = $property->getHook(PropertyHookType::Set);
$paramType = $setHook->getParameters()[0]->getType();
echo ":"; echo count($paramType->getTypes());
$box = new EvalReflectSettableTypeTarget();
$box->value = 7;
echo ":"; echo $box->value;
return true;"##,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "string:2:intB:stringB:2:7");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionProperty exposes eval property default metadata.
#[test]
fn execute_program_reflection_property_get_default_value_metadata() {
    let program = parse_fragment(
        br#"class EvalReflectPropertyDefaultTarget {
    public $implicit;
    public int $typed;
    public ?string $nullableTyped;
    public $explicitNull = null;
    public int $count = 7;
    public static string $label = "ok";
}

foreach (["implicit", "typed", "nullableTyped", "explicitNull", "count", "label"] as $name) {
    $property = new ReflectionProperty("EvalReflectPropertyDefaultTarget", $name);
    echo $property->getName(); echo ":";
    echo $property->isDefault() ? "Y:" : "N:";
    echo $property->hasDefaultValue() ? "D:" : "d:";
    $value = $property->getDefaultValue();
    echo $value === null ? "null" : $value;
    echo "|";
}
$listed = (new ReflectionClass("EvalReflectPropertyDefaultTarget"))->getProperty("implicit");
echo "listed:";
echo $listed->isDefault() ? "Y:" : "N:";
echo $listed->hasDefaultValue() ? "D:" : "d:";
echo $listed->getDefaultValue() === null ? "null" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "implicit:Y:D:null|typed:Y:d:null|nullableTyped:Y:d:null|explicitNull:Y:D:null|count:Y:D:7|label:Y:D:ok|listed:Y:D:null"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionProperty formats retained property metadata through `__toString()`.
#[test]
fn execute_program_reflection_property_to_string() {
    let program = parse_fragment(
        br#"class EvalReflectPropertyStringTarget {
    public int $id = 7;
    protected static string $label = "ok";
    private $implicit;
    public $virtual {
        get => 1;
    }
}
foreach (["id", "label", "implicit", "virtual"] as $name) {
    echo (new ReflectionProperty("EvalReflectPropertyStringTarget", $name))->__toString();
    echo "|";
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Property [ public int $id = 7 ]|Property [ protected static string $label = 'ok' ]|Property [ private $implicit = NULL ]|Property [ public $virtual ]|"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
