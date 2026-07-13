//! Purpose:
//! Interpreter tests for Reflection attributes, origins, sources, and prototypes.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Coverage includes eval-declared attributes and callable metadata provenance.

use super::super::super::*;
use super::super::support::*;

/// Verifies class attribute helpers expose eval class-level metadata.
#[test]
fn execute_program_dispatches_class_attribute_metadata_builtins() {
    let program = parse_fragment(
        br#"class EvalAttrDep {}
#[Route("/home", -1, 1.5, true, null, EvalAttrDep::class, ["nested", 2])]
#[Tag("first"), Tag("second")]
class EvalAttrMeta {}
$names = class_attribute_names("EvalAttrMeta");
echo count($names); echo ":"; echo $names[0]; echo ":"; echo $names[1]; echo ":"; echo $names[2]; echo ":";
$args = class_attribute_args("EvalAttrMeta", "route");
echo count($args); echo ":"; echo $args[0]; echo ":"; echo $args[1]; echo ":";
echo $args[2]; echo ":"; echo $args[3] ? "T" : "F"; echo ":"; echo is_null($args[4]) ? "N" : "bad"; echo ":";
echo $args[5]; echo ":";
echo count($args[6]); echo ":"; echo $args[6][0]; echo ":"; echo $args[6][1]; echo ":";
$tag = class_attribute_args("evalattrmeta", "Tag");
echo $tag[0]; echo ":";
$missing = class_attribute_args("EvalAttrMeta", "Missing");
echo count($missing); echo ":";
$attrs = class_get_attributes("EvalAttrMeta");
echo count($attrs); echo ":"; echo $attrs[0]->getName(); echo ":";
$attr_args = $attrs[0]->getArguments();
echo count($attr_args); echo ":"; echo $attr_args[0]; echo ":"; echo $attr_args[1]; echo ":";
echo $attr_args[2]; echo ":"; echo $attr_args[3] ? "T" : "F"; echo ":"; echo is_null($attr_args[4]) ? "N" : "bad"; echo ":";
echo $attr_args[5]; echo ":";
echo count($attr_args[6]); echo ":"; echo $attr_args[6][0]; echo ":"; echo $attr_args[6][1]; echo ":";
$tag_attr_args = $attrs[1]->getArguments();
echo $attrs[1]->getName(); echo ":"; echo $tag_attr_args[0]; echo ":";
echo is_null($attrs[0]->newInstance()) ? "N" : "bad"; echo ":";
$call_names = call_user_func("class_attribute_names", "EvalAttrMeta");
echo $call_names[0]; echo ":";
$call_args = call_user_func_array(
    "class_attribute_args",
    ["class_name" => "EvalAttrMeta", "attribute_name" => "Route"]
);
echo $call_args[0]; echo ":";
echo function_exists("class_attribute_names"); echo function_exists("class_get_attributes");
echo function_exists("class_attribute_args");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "3:Route:Tag:Tag:7:/home:-1:1.5:T:N:EvalAttrDep:2:nested:2:first:0:3:Route:7:/home:-1:1.5:T:N:EvalAttrDep:2:nested:2:Tag:first:N:Route:/home:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval class attribute metadata preserves named literal arguments.
#[test]
fn execute_program_dispatches_named_class_attribute_args() {
    let program = parse_fragment(
        br#"#[Route(path: "/eval", secure: true, code: 9)]
class EvalNamedAttrMeta {}
$args = class_attribute_args("EvalNamedAttrMeta", "Route");
echo count($args); echo ":";
echo $args["path"]; echo ":";
echo $args["secure"] ? "T" : "F"; echo ":";
echo $args["code"]; echo ":";
$attrs = class_get_attributes("EvalNamedAttrMeta");
$attr_args = $attrs[0]->getArguments();
echo $attr_args["path"]; echo ":";
echo $attr_args["secure"] ? "T" : "F"; echo ":";
echo $attr_args["code"];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:/eval:T:9:/eval:T:9");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionAttribute::newInstance instantiates eval-declared attribute classes.
#[test]
fn execute_program_instantiates_eval_declared_reflection_attribute() {
    let program = parse_fragment(
        br#"class EvalRoute {
    public $path;
    public $code;
    public $enabled;
    public function __construct($path, $code, $enabled) {
        $this->path = $path;
        $this->code = $code;
        $this->enabled = $enabled;
    }
    public function summary() {
        return $this->path . ":" . $this->code . ":" . ($this->enabled ? "T" : "F");
    }
}
#[EvalRoute(enabled: true, code: -7, path: "/home")]
class EvalRouteTarget {}
$attrs = class_get_attributes("EvalRouteTarget");
$instance = $attrs[0]->newInstance();
echo get_class($instance); echo ":"; echo $instance->summary();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "EvalRoute:/home:-7:T");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass/Method/Property expose eval-declared attribute metadata.
#[test]
fn execute_program_reflects_eval_member_attributes() {
    let program = parse_fragment(
        br#"class EvalMarker {
    public $name;
    public function __construct($name) {
        $this->name = $name;
    }
    public function label() {
        return $this->name;
    }
}
#[EvalMarker("class")]
class EvalReflectTarget {
    #[EvalMarker("method")]
    public function handle() {}
    #[EvalMarker("property")]
    public $id;
}
$class_attrs = (new ReflectionClass("EvalReflectTarget"))->getAttributes();
echo count($class_attrs); echo ":"; echo (new ReflectionClass("EvalReflectTarget"))->getName(); echo ":";
echo $class_attrs[0]->getName(); echo ":"; echo $class_attrs[0]->newInstance()->label(); echo ":";
$method_attrs = (new ReflectionMethod("EvalReflectTarget", "handle"))->getAttributes();
echo count($method_attrs); echo ":"; echo (new ReflectionMethod("EvalReflectTarget", "handle"))->getName(); echo ":";
echo $method_attrs[0]->getName(); echo ":";
echo $method_attrs[0]->getArguments()[0]; echo ":"; echo $method_attrs[0]->newInstance()->label(); echo ":";
$property_attrs = (new ReflectionProperty("EvalReflectTarget", "id"))->getAttributes();
echo count($property_attrs); echo ":"; echo (new ReflectionProperty("EvalReflectTarget", "id"))->getName(); echo ":";
echo $property_attrs[0]->getName(); echo ":";
echo $property_attrs[0]->getArguments()[0]; echo ":"; echo $property_attrs[0]->newInstance()->label();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:EvalReflectTarget:EvalMarker:class:1:handle:EvalMarker:method:method:1:id:EvalMarker:property:property"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionAttribute reports target bitmasks and repeated-owner metadata.
#[test]
fn execute_program_reflection_attribute_reports_target_and_repetition() {
    let program = parse_fragment(
        br#"class EvalTargetMarker {
    public function __construct($name = null) {}
}
#[EvalTargetMarker("class-a"), EvalTargetMarker("class-b")]
class EvalReflectAttributeTarget {
    #[EvalTargetMarker("method")]
    public function run(#[EvalTargetMarker("param")] $id) {}
    #[EvalTargetMarker("property")]
    public $id;
    #[EvalTargetMarker("const")]
    public const ANSWER = 42;
}
enum EvalReflectAttributeEnum {
    #[EvalTargetMarker("case")]
    case Ready;
}
$class_attrs = (new ReflectionClass("EvalReflectAttributeTarget"))->getAttributes();
echo $class_attrs[0]->getTarget(); echo "/";
echo $class_attrs[0]->isRepeated() ? "R" : "r"; echo ":";
echo $class_attrs[1]->getTarget(); echo "/";
echo $class_attrs[1]->isRepeated() ? "R" : "r"; echo ":";
$method_attr = (new ReflectionMethod("EvalReflectAttributeTarget", "run"))->getAttributes()[0];
echo $method_attr->getTarget(); echo "/";
echo $method_attr->isRepeated() ? "R" : "r"; echo ":";
$property_attr = (new ReflectionProperty("EvalReflectAttributeTarget", "id"))->getAttributes()[0];
echo $property_attr->getTarget(); echo "/";
echo $property_attr->isRepeated() ? "R" : "r"; echo ":";
$param_attr = (new ReflectionMethod("EvalReflectAttributeTarget", "run"))->getParameters()[0]->getAttributes()[0];
echo $param_attr->getTarget(); echo "/";
echo $param_attr->isRepeated() ? "R" : "r"; echo ":";
$const_attr = (new ReflectionClassConstant("EvalReflectAttributeTarget", "ANSWER"))->getAttributes()[0];
echo $const_attr->getTarget(); echo "/";
echo $const_attr->isRepeated() ? "R" : "r"; echo ":";
$case_attr = (new ReflectionEnumUnitCase("EvalReflectAttributeEnum", "Ready"))->getAttributes()[0];
echo $case_attr->getTarget(); echo "/";
echo $case_attr->isRepeated() ? "R" : "r";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1/R:1/R:4/r:8/r:32/r:16/r:16/r");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies reflection owner origin metadata APIs report eval user-defined defaults.
#[test]
fn execute_program_reflection_owners_report_origin_metadata_defaults() {
    let program = parse_fragment(
        br#"class EvalReflectOriginTarget {
    public $id;
    public const ANSWER = 42;
    public function run() {}
}
enum EvalReflectOriginCase: string {
    case Ready = "ready";
}
$class = new ReflectionClass("EvalReflectOriginTarget");
$method = new ReflectionMethod("EvalReflectOriginTarget", "run");
$property = new ReflectionProperty("EvalReflectOriginTarget", "id");
$constant = new ReflectionClassConstant("EvalReflectOriginTarget", "ANSWER");
$unit = new ReflectionEnumUnitCase("EvalReflectOriginCase", "Ready");
$backed = new ReflectionEnumBackedCase("EvalReflectOriginCase", "Ready");
echo ($class->getDocComment() === false) ? "C" : "c"; echo ":";
echo ($method->getDocComment() === false) ? "M" : "m"; echo ":";
echo ($property->getDocComment() === false) ? "P" : "p"; echo ":";
echo ($constant->getDocComment() === false) ? "K" : "k"; echo ":";
echo ($unit->getDocComment() === false) ? "U" : "u"; echo ":";
echo ($backed->getDocComment() === false) ? "B" : "b"; echo ":";
echo ($class->getExtensionName() === false) ? "E" : "e"; echo ":";
echo ($method->getExtensionName() === false) ? "N" : "n"; echo ":";
echo ($class->getExtension() === null) ? "X" : "x"; echo ":";
echo ($method->getExtension() === null) ? "Y" : "y";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "C:M:P:K:U:B:E:N:X:Y");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass and ReflectionMethod report eval source-location metadata.
#[test]
fn execute_program_reflection_class_and_method_report_source_location() {
    let program = parse_fragment(
        br#"class EvalReflectSource {
    public function run() {
        return 1;
    }
}
interface EvalReflectSourceIface {
    public function iface();
}
$class = new ReflectionClass("EvalReflectSource");
$method = new ReflectionMethod("EvalReflectSource", "run");
$iface = new ReflectionClass("EvalReflectSourceIface");
$ifaceMethod = new ReflectionMethod("EvalReflectSourceIface", "iface");
echo $class->getFileName(); echo ":";
echo $class->getStartLine(); echo ":"; echo $class->getEndLine(); echo ":";
echo $method->getStartLine(); echo ":"; echo $method->getEndLine(); echo ":";
echo $iface->getStartLine(); echo ":"; echo $iface->getEndLine(); echo ":";
echo $ifaceMethod->getStartLine(); echo ":"; echo $ifaceMethod->getEndLine();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/eval-class-source.php", "/tmp", 23);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(
        values.output,
        "/tmp/eval-class-source.php(23) : eval()'d code:1:5:2:4:6:8:7:7"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod exposes PHP-compatible name and origin predicate metadata.
#[test]
fn execute_program_reflection_method_reports_name_and_origin_predicates() {
    let program = parse_fragment(
        br#"namespace EvalReflectMethodNs;
class Target {
    public function run(...$items) {}
    public static function stat() {}
}
$ref = new \ReflectionMethod(Target::class, "run");
echo $ref->getShortName(); echo ":";
echo $ref->getNamespaceName(); echo ":";
echo $ref->inNamespace() ? "Y" : "N"; echo ":";
echo $ref->isInternal() ? "I" : "i";
echo $ref->isUserDefined() ? "U" : "u"; echo ":";
echo $ref->isClosure() ? "C" : "c"; echo ":";
echo $ref->isDeprecated() ? "D" : "d"; echo ":";
echo $ref->isStatic() ? "S" : "s"; echo ":";
echo $ref->returnsReference() ? "R" : "r"; echo ":";
echo $ref->hasReturnType() ? "T" : "t"; echo ":";
echo $ref->getReturnType() === null ? "N" : "n"; echo ":";
echo $ref->isGenerator() ? "G" : "g"; echo ":";
echo $ref->isVariadic() ? "V" : "v"; echo ":";
echo $ref->hasTentativeReturnType() ? "H" : "h"; echo ":";
echo $ref->getTentativeReturnType() === null ? "Q" : "q"; echo ":";
echo count($ref->getClosureUsedVariables()); echo ":";
echo $ref->getClosureThis() === null ? "T" : "t"; echo ":";
echo $ref->getClosureScopeClass() === null ? "S" : "s"; echo ":";
echo $ref->getClosureCalledClass() === null ? "L" : "l"; echo ":";
$static = new \ReflectionMethod(Target::class, "stat");
echo $static->isStatic() ? "S" : "s";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "run::N:iU:c:d:s:r:t:N:g:V:h:Q:0:T:S:L:S");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod derives `isDeprecated()` from eval-retained attributes.
#[test]
fn execute_program_reflection_method_reports_deprecated_attribute() {
    let program = parse_fragment(
        br#"class EvalReflectDeprecatedMethodTarget {
    #[\Deprecated]
    public function old() {}
    public function fresh() {}
}
$deprecated = new ReflectionMethod(EvalReflectDeprecatedMethodTarget::class, "old");
$plain = new ReflectionMethod(EvalReflectDeprecatedMethodTarget::class, "fresh");
echo $deprecated->isDeprecated() ? "D" : "d"; echo ":";
echo $plain->isDeprecated() ? "D" : "d";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "D:d");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod exposes eval static locals using the declaring class key.
#[test]
fn execute_program_reflection_method_reports_static_variables() {
    let program = parse_fragment(
        br#"class EvalReflectMethodStaticBase {
    public function tick() {
        static $count = 3;
        static $label = "method";
        $count = $count + 1;
        return $count;
    }
}
class EvalReflectMethodStaticChild extends EvalReflectMethodStaticBase {}
$object = new EvalReflectMethodStaticChild();
$ref = new ReflectionMethod("EvalReflectMethodStaticChild", "tick");
$before = $ref->getStaticVariables();
echo $before["count"]; echo ":"; echo $before["label"]; echo ":";
echo $ref->invoke($object); echo ":";
$after = $ref->getStaticVariables();
echo $after["count"]; echo ":"; echo $after["label"];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:method:4:4:method");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod exposes eval parent and interface prototypes.
#[test]
fn execute_program_reflection_method_reports_eval_prototypes() {
    let program = parse_fragment(
        br#"interface EvalProtoParentIface {
    public function parented();
}
interface EvalProtoChildIface extends EvalProtoParentIface {}
interface EvalProtoIface {
    public function iface();
}
class EvalProtoBase {
    public function run() {}
    public function inherited() {}
}
class EvalProtoChild extends EvalProtoBase implements EvalProtoIface, EvalProtoChildIface {
    public function run() {}
    public function iface() {}
    public function parented() {}
    public function own() {}
}
$override = new ReflectionMethod("EvalProtoChild", "run");
$overrideProto = $override->getPrototype();
echo $override->hasPrototype() ? "Y" : "N"; echo ":";
echo $overrideProto->getDeclaringClass()->getName(); echo "::";
echo $overrideProto->getName(); echo ":";
$iface = new ReflectionMethod("EvalProtoChild", "iface");
$ifaceProto = $iface->getPrototype();
echo $iface->hasPrototype() ? "Y" : "N"; echo ":";
echo $ifaceProto->getDeclaringClass()->getName(); echo "::";
echo $ifaceProto->getName(); echo ":";
$parentIface = new ReflectionMethod("EvalProtoChild", "parented");
$parentIfaceProto = $parentIface->getPrototype();
echo $parentIfaceProto->getDeclaringClass()->getName(); echo "::";
echo $parentIfaceProto->getName(); echo ":";
$own = new ReflectionMethod("EvalProtoChild", "own");
echo $own->hasPrototype() ? "Y" : "N"; echo ":";
try {
    $own->getPrototype();
} catch (ReflectionException $e) {
    echo "E";
}
echo ":";
$inherited = new ReflectionMethod("EvalProtoChild", "inherited");
echo $inherited->hasPrototype() ? "Y" : "N";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Y:EvalProtoBase::run:Y:EvalProtoIface::iface:EvalProtoParentIface::parented:N:E:N"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
