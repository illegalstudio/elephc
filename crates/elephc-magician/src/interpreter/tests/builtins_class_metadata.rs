//! Purpose:
//! Interpreter tests for eval class metadata and relation builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Eval class declarations expose parent/interface metadata plus class-level
//!   attribute names and supported literal positional args.
//! - Tests verify direct calls, dynamic calls, named arguments, and builtin probes.

use super::super::*;
use super::support::*;

/// Verifies class-relation helpers return empty arrays for known eval classes.
#[test]
fn execute_program_dispatches_class_relation_builtins() {
    let program = parse_fragment(
        br#"class EvalMeta {}
$object = new EvalMeta();
$implements = class_implements("EvalMeta");
echo is_array($implements) && count($implements) === 0 ? "impl" : "bad"; echo ":";
$parents = class_parents($object);
echo is_array($parents) && count($parents) === 0 ? "parents" : "bad"; echo ":";
$uses = class_uses("EvalMeta");
echo is_array($uses) && count($uses) === 0 ? "uses" : "bad"; echo ":";
echo class_implements("MissingMeta") === false ? "missing" : "bad"; echo ":";
$call = call_user_func("class_implements", "EvalMeta");
echo is_array($call) && count($call) === 0 ? "call" : "bad"; echo ":";
$named = call_user_func_array("class_parents", ["object_or_class" => "EvalMeta"]);
echo is_array($named) && count($named) === 0 ? "named" : "bad"; echo ":";
echo function_exists("class_implements"); echo function_exists("class_parents");
echo function_exists("class_uses");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "impl:parents:uses:missing:call:named:111");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval-declared parent and interface metadata is exposed to relation builtins.
#[test]
fn execute_program_reports_eval_class_relation_metadata() {
    let program = parse_fragment(
        br#"class EvalMetaBase {}
class EvalMetaChild extends EvalMetaBase implements KnownInterface {}
$object = new EvalMetaChild();
$implements = class_implements($object);
echo count($implements); echo ":";
echo $implements["KnownInterface"]; echo ":";
$parents = class_parents("EvalMetaChild");
echo count($parents); echo ":";
echo $parents["EvalMetaBase"]; echo ":";
$call = call_user_func("class_implements", "EvalMetaChild");
echo $call["KnownInterface"]; echo ":";
$named = call_user_func_array("class_parents", ["object_or_class" => $object]);
echo $named["EvalMetaBase"];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:KnownInterface:1:EvalMetaBase:KnownInterface:EvalMetaBase"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies PHP OOP introspection builtins follow eval visibility and scope rules.
#[test]
fn execute_program_dispatches_oop_introspection_builtins() {
    let program = parse_fragment(
        br#"class EvalOopIntrospectBase {
    private $baseSecret = "bp";
    protected $baseProtected = "bq";
    public $basePublic = "br";
    private function basePrivate() {}
    protected function baseProtectedMethod() {}
    public function basePublicMethod() {}
    public function parentView() {
        $vars = get_object_vars($this);
        ksort($vars);
        echo implode(",", array_keys($vars));
    }
}
class EvalOopIntrospectChild extends EvalOopIntrospectBase {
    private $childSecret = "cp";
    protected $childProtected = "cq";
    public $childPublic = "cr";
    private function childPrivate() {}
    protected function childProtectedMethod() {}
    public function childPublicMethod() {}
    public function childView() {
        $methods = get_class_methods($this);
        sort($methods);
        echo implode(",", $methods); echo "|";
        $vars = get_object_vars($this);
        ksort($vars);
        echo implode(",", array_keys($vars));
    }
}
$object = new EvalOopIntrospectChild();
$object->dynamic = "dyn";
echo method_exists("EvalOopIntrospectChild", "basePrivate") ? "bad" : "noParentPrivateMethod"; echo ":";
echo method_exists($object, "basePrivate") ? "objectParentPrivateMethod" : "bad"; echo ":";
echo method_exists("EvalOopIntrospectChild", "baseProtectedMethod") ? "classProtectedMethod" : "bad"; echo ":";
echo property_exists("EvalOopIntrospectChild", "baseSecret") ? "bad" : "noParentPrivateProperty"; echo ":";
echo property_exists($object, "baseSecret") ? "bad" : "noObjectParentPrivateProperty"; echo ":";
echo property_exists($object, "dynamic") ? "dynamicProperty" : "bad"; echo ":";
$methods = get_class_methods("EvalOopIntrospectChild");
sort($methods);
echo implode(",", $methods); echo ":";
$vars = get_object_vars($object);
ksort($vars);
echo implode(",", array_keys($vars)); echo ":";
$object->childView(); echo ":";
$object->parentView(); echo ":";
echo call_user_func("method_exists", $object, "childPrivate") ? "callMethod" : "bad"; echo ":";
echo call_user_func_array("property_exists", ["property" => "dynamic", "object_or_class" => $object]) ? "namedProperty" : "bad"; echo ":";
echo function_exists("method_exists"); echo function_exists("property_exists");
echo function_exists("get_class_methods"); echo function_exists("get_object_vars");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "noParentPrivateMethod:objectParentPrivateMethod:classProtectedMethod:noParentPrivateProperty:noObjectParentPrivateProperty:dynamicProperty:basePublicMethod,childPublicMethod,childView,parentView:basePublic,childPublic,dynamic:baseProtectedMethod,basePublicMethod,childPrivate,childProtectedMethod,childPublicMethod,childView,parentView|baseProtected,basePublic,childProtected,childPublic,childSecret,dynamic:baseProtected,basePublic,baseSecret,childProtected,childPublic,dynamic:callMethod:namedProperty:1111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies class attribute helpers expose eval class-level metadata.
#[test]
fn execute_program_dispatches_class_attribute_metadata_builtins() {
    let program = parse_fragment(
        br#"#[Route("/home", -1, true, null)]
#[Tag("first"), Tag("second")]
class EvalAttrMeta {}
$names = class_attribute_names("EvalAttrMeta");
echo count($names); echo ":"; echo $names[0]; echo ":"; echo $names[1]; echo ":"; echo $names[2]; echo ":";
$args = class_attribute_args("EvalAttrMeta", "route");
echo count($args); echo ":"; echo $args[0]; echo ":"; echo $args[1]; echo ":";
echo $args[2] ? "T" : "F"; echo ":"; echo is_null($args[3]) ? "N" : "bad"; echo ":";
$tag = class_attribute_args("evalattrmeta", "Tag");
echo $tag[0]; echo ":";
$missing = class_attribute_args("EvalAttrMeta", "Missing");
echo count($missing); echo ":";
$attrs = class_get_attributes("EvalAttrMeta");
echo count($attrs); echo ":"; echo $attrs[0]->getName(); echo ":";
$attr_args = $attrs[0]->getArguments();
echo count($attr_args); echo ":"; echo $attr_args[0]; echo ":"; echo $attr_args[1]; echo ":";
echo $attr_args[2] ? "T" : "F"; echo ":"; echo is_null($attr_args[3]) ? "N" : "bad"; echo ":";
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
        "3:Route:Tag:Tag:4:/home:-1:T:N:first:0:3:Route:4:/home:-1:T:N:Tag:first:N:Route:/home:111"
    );
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
#[EvalRoute("/home", -7, true)]
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
}
$ref = new \ReflectionMethod(Target::class, "run");
echo $ref->getShortName(); echo ":";
echo $ref->getNamespaceName(); echo ":";
echo $ref->inNamespace() ? "Y" : "N"; echo ":";
echo $ref->isInternal() ? "I" : "i";
echo $ref->isUserDefined() ? "U" : "u"; echo ":";
echo $ref->isClosure() ? "C" : "c"; echo ":";
echo $ref->isDeprecated() ? "D" : "d"; echo ":";
echo $ref->returnsReference() ? "R" : "r"; echo ":";
echo $ref->hasReturnType() ? "T" : "t"; echo ":";
echo $ref->getReturnType() === null ? "N" : "n"; echo ":";
echo $ref->isGenerator() ? "G" : "g"; echo ":";
echo $ref->isVariadic() ? "V" : "v"; echo ":";
echo $ref->hasTentativeReturnType() ? "H" : "h"; echo ":";
echo $ref->getTentativeReturnType() === null ? "Q" : "q"; echo ":";
echo count($ref->getClosureUsedVariables());
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "run::N:iU:c:d:r:t:N:g:V:h:Q:0");
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

/// Verifies ReflectionClass exposes eval class namespace-derived name parts.
#[test]
fn execute_program_reflects_eval_class_name_parts() {
    let program = parse_fragment(
        br#"namespace Eval\Ns;
class Thing {}
$ref = new \ReflectionClass("Eval\\Ns\\Thing");
echo $ref->getName(); echo ":";
echo $ref->getShortName(); echo ":";
echo $ref->getNamespaceName(); echo ":";
echo $ref->inNamespace() ? "Y" : "N";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Eval\\Ns\\Thing:Thing:Eval\\Ns:Y");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass exposes eval interface and trait relation names.
#[test]
fn execute_program_reflects_eval_class_relation_names() {
    let program = parse_fragment(
        br#"interface EvalRelationIface {}
trait EvalRelationTrait {
    public function primary() {}
}
trait EvalRelationOtherTrait {
    public function other() {}
}
class EvalRelationTarget implements EvalRelationIface {
    use EvalRelationTrait, EvalRelationOtherTrait {
        EvalRelationTrait::primary as relationAlias;
        EvalRelationOtherTrait::other as private hiddenOther;
        EvalRelationOtherTrait::other as protected;
    }
}
class EvalRelationInherited extends EvalRelationTarget {}
interface EvalRelationParent {}
interface EvalRelationChild extends EvalRelationParent {}
$ref = new ReflectionClass("EvalRelationTarget");
$interfaces = $ref->getInterfaceNames();
$traits = $ref->getTraitNames();
echo count($interfaces); echo ":"; echo $interfaces[0]; echo ":";
echo count($traits); echo ":"; echo $traits[0]; echo ":"; echo $traits[1]; echo ":";
$parentInterfaces = (new ReflectionClass("EvalRelationChild"))->getInterfaceNames();
echo count($parentInterfaces); echo ":"; echo $parentInterfaces[0];
$interfaceObjects = $ref->getInterfaces();
echo ":"; echo count($interfaceObjects); echo ":"; echo $interfaceObjects["EvalRelationIface"]->getName();
$traitObjects = $ref->getTraits();
echo ":"; echo count($traitObjects); echo ":"; echo $traitObjects["EvalRelationTrait"]->getName(); echo ":"; echo $traitObjects["EvalRelationOtherTrait"]->getName();
$parentInterfaceObjects = (new ReflectionClass("EvalRelationChild"))->getInterfaces();
echo ":"; echo count($parentInterfaceObjects); echo ":"; echo $parentInterfaceObjects["EvalRelationParent"]->getName();
$aliases = $ref->getTraitAliases();
echo ":"; echo count($aliases); echo ":"; echo $aliases["relationAlias"]; echo ":"; echo $aliases["hiddenOther"];
$inheritedAliases = (new ReflectionClass("EvalRelationInherited"))->getTraitAliases();
echo ":"; echo count($inheritedAliases);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:EvalRelationIface:2:EvalRelationTrait:EvalRelationOtherTrait:1:EvalRelationParent:1:EvalRelationIface:2:EvalRelationTrait:EvalRelationOtherTrait:1:EvalRelationParent:2:EvalRelationTrait::primary:EvalRelationOtherTrait::other:0"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::getInterfaceNames reads fake generated/AOT metadata.
#[test]
fn execute_program_reflects_aot_class_interface_names() {
    let program = parse_fragment(
        br#"$class_names = (new ReflectionClass("KnownClass"))->getInterfaceNames();
echo count($class_names); echo ":"; echo $class_names[0]; echo ":";
$interface_names = (new ReflectionClass("KnownInterface"))->getInterfaceNames();
echo count($interface_names); echo ":"; echo $interface_names[0]; echo ":";
$class_objects = (new ReflectionClass("KnownClass"))->getInterfaces();
echo count($class_objects); echo ":"; echo $class_objects["KnownInterface"]->getName(); echo ":";
$interface_objects = (new ReflectionClass("KnownInterface"))->getInterfaces();
echo count($interface_objects); echo ":"; echo $interface_objects["Traversable"]->getName();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:KnownInterface:1:Traversable:1:KnownInterface:1:Traversable"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::implementsInterface reports eval class, enum, and
/// interface metadata using case-insensitive interface names.
#[test]
fn execute_program_reflects_eval_class_implements_interface_predicate() {
    let program = parse_fragment(
        br#"interface EvalImplBase {}
interface EvalImplChild extends EvalImplBase {}
class EvalImplTarget implements EvalImplChild {}
enum EvalImplEnum implements EvalImplBase { case Ready; }
trait EvalImplTrait {}
echo (new ReflectionClass("EvalImplTarget"))->implementsInterface("EvalImplChild") ? "C" : "c";
echo (new ReflectionClass("EvalImplTarget"))->implementsInterface("evalimplbase") ? "B" : "b";
echo (new ReflectionClass("EvalImplEnum"))->implementsInterface("EvalImplBase") ? "E" : "e";
echo (new ReflectionClass("EvalImplChild"))->implementsInterface("EvalImplChild") ? "I" : "i";
echo (new ReflectionClass("EvalImplChild"))->implementsInterface("EvalImplBase") ? "P" : "p";
echo (new ReflectionClass("EvalImplTrait"))->implementsInterface("EvalImplBase") ? "T" : "t";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "CBEIPt");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::implementsInterface checks fake generated/AOT relations.
#[test]
fn execute_program_reflects_aot_class_implements_interface_predicate() {
    let program = parse_fragment(
        br#"$ref = new ReflectionClass("KnownClass");
echo $ref->implementsInterface("KnownInterface") ? "Y" : "N"; echo ":";
echo $ref->implementsInterface("Iterator") ? "bad" : "N";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Y:N");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::implementsInterface rejects non-interface names with catchable errors.
#[test]
fn execute_program_reflection_class_implements_interface_rejects_non_interfaces() {
    let program = parse_fragment(
        br#"interface EvalImplRejectIface {}
class EvalImplRejectTarget {}
class EvalImplRejectClass {}
trait EvalImplRejectTrait {}
enum EvalImplRejectEnum { case Ready; }
$ref = new ReflectionClass("EvalImplRejectTarget");
echo $ref->implementsInterface("EvalImplRejectIface") ? "T" : "F";
try {
    $ref->implementsInterface("EvalImplRejectClass");
    echo ":bad";
} catch (ReflectionException $e) {
    echo ":"; echo get_class($e); echo ":"; echo $e->getMessage();
}
try {
    $ref->implementsInterface("EvalImplRejectTrait");
    echo ":bad";
} catch (ReflectionException $e) {
    echo ":"; echo get_class($e); echo ":"; echo $e->getMessage();
}
try {
    $ref->implementsInterface("EvalImplRejectEnum");
    echo ":bad";
} catch (ReflectionException $e) {
    echo ":"; echo get_class($e); echo ":"; echo $e->getMessage();
}
try {
    $ref->implementsInterface("EvalImplRejectMissing");
    echo ":bad";
} catch (ReflectionException $e) {
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
        "F:ReflectionException:EvalImplRejectClass is not an interface:ReflectionException:EvalImplRejectTrait is not an interface:ReflectionException:EvalImplRejectEnum is not an interface:ReflectionException:Interface \"EvalImplRejectMissing\" does not exist"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::isSubclassOf reports eval parent/interface metadata.
#[test]
fn execute_program_reflection_class_is_subclass_of_predicate() {
    let program = parse_fragment(
        br#"interface EvalSubclassIface {}
interface EvalSubclassChildIface extends EvalSubclassIface {}
class EvalSubclassBase {}
class EvalSubclassParent extends EvalSubclassBase {}
class EvalSubclassChild extends EvalSubclassParent implements EvalSubclassChildIface {}
trait EvalSubclassTrait {}
enum EvalSubclassEnum implements EvalSubclassIface { case Ready; }
$ref = new ReflectionClass("EvalSubclassChild");
echo $ref->isSubclassOf("EvalSubclassParent") ? "P" : "p";
echo $ref->isSubclassOf("evalsubclassbase") ? "B" : "b";
echo $ref->isSubclassOf("EvalSubclassIface") ? "I" : "i";
echo $ref->isSubclassOf("EvalSubclassChild") ? "S" : "s";
echo (new ReflectionClass("EvalSubclassChildIface"))->isSubclassOf("EvalSubclassIface") ? "J" : "j";
echo (new ReflectionClass("EvalSubclassIface"))->isSubclassOf("EvalSubclassIface") ? "X" : "x";
echo $ref->isSubclassOf("EvalSubclassTrait") ? "T" : "t";
echo $ref->isSubclassOf("EvalSubclassEnum") ? "Q" : "q";
echo (new ReflectionClass("EvalSubclassEnum"))->isSubclassOf("EvalSubclassIface") ? "E" : "e";
try {
    $ref->isSubclassOf("EvalSubclassMissing");
    echo ":bad";
} catch (ReflectionException $e) {
    echo ":missing";
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "PBIsJxtqE:missing");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::isInstance reports eval object class/interface metadata.
#[test]
fn execute_program_reflection_class_is_instance_predicate() {
    let program = parse_fragment(
        br#"interface EvalInstanceIface {}
class EvalInstanceBase {}
class EvalInstanceChild extends EvalInstanceBase implements EvalInstanceIface {}
trait EvalInstanceTrait {}
enum EvalInstanceEnum implements EvalInstanceIface { case Ready; }
$base = new ReflectionClass("EvalInstanceBase");
$child = new ReflectionClass("EvalInstanceChild");
$iface = new ReflectionClass("EvalInstanceIface");
$trait = new ReflectionClass("EvalInstanceTrait");
$enum = new ReflectionClass("EvalInstanceEnum");
$childObj = new EvalInstanceChild();
$objectRef = new ReflectionClass($childObj);
echo $objectRef->getName(); echo ":";
echo $objectRef->getParentClass()->getName(); echo ":";
echo $objectRef->isInstance($childObj) ? "O" : "o"; echo ":";
echo $base->isInstance($childObj) ? "B" : "b";
echo $child->isInstance(new EvalInstanceBase()) ? "C" : "c";
echo $iface->isInstance($childObj) ? "I" : "i";
echo $trait->isInstance($childObj) ? "T" : "t";
echo $enum->isInstance(EvalInstanceEnum::Ready) ? "E" : "e";
echo $iface->isInstance(EvalInstanceEnum::Ready) ? "N" : "n";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "EvalInstanceChild:EvalInstanceBase:O:BcItEN");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass exposes eval class-like final and abstract flags.
#[test]
fn execute_program_reflects_eval_class_modifier_flags() {
    let program = parse_fragment(
        br#"abstract class EvalAbstractReflect {}
final class EvalFinalReflect {}
interface EvalIfaceReflect {}
trait EvalTraitReflect {}
enum EvalEnumReflect { case Ready; }
echo (new ReflectionClass("EvalAbstractReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalAbstractReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalAbstractReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalAbstractReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalAbstractReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalFinalReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalFinalReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalFinalReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalFinalReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalFinalReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalEnumReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalEnumReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalEnumReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalEnumReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalEnumReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalIfaceReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalIfaceReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalIfaceReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalIfaceReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalIfaceReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalTraitReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalTraitReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalTraitReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalTraitReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalTraitReflect"))->isEnum() ? "E" : "e";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Afite:aFite:aFitE:afIte:afiTe");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass exposes PHP modifier bitmasks for eval class-like metadata.
#[test]
fn execute_program_reflects_eval_class_modifier_bitmask() {
    let program = parse_fragment(
        br#"abstract class EvalModifierAbstract {}
final class EvalModifierFinal {}
readonly class EvalModifierReadonly {}
final readonly class EvalModifierFinalReadonly {}
enum EvalModifierEnum { case Ready; }
interface EvalModifierIface {}
trait EvalModifierTrait {}
echo (new ReflectionClass("EvalModifierAbstract"))->getModifiers(); echo ":";
echo (new ReflectionClass("EvalModifierFinal"))->getModifiers(); echo ":";
echo (new ReflectionClass("EvalModifierReadonly"))->getModifiers(); echo ":";
echo (new ReflectionClass("EvalModifierFinalReadonly"))->getModifiers(); echo ":";
echo (new ReflectionClass("EvalModifierEnum"))->getModifiers(); echo ":";
echo (new ReflectionClass("EvalModifierIface"))->getModifiers(); echo ":";
echo (new ReflectionClass("EvalModifierTrait"))->getModifiers();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "64:32:65536:65568:32:0:0");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval can read built-in Reflection `IS_*` class constants.
#[test]
fn execute_program_reads_builtin_reflection_modifier_constants() {
    let program = parse_fragment(
        br#"echo ReflectionClass::IS_FINAL; echo ":";
echo ReflectionClass::IS_EXPLICIT_ABSTRACT; echo ":";
echo ReflectionClass::IS_READONLY; echo ":";
echo ReflectionMethod::IS_STATIC; echo ":";
echo ReflectionMethod::IS_PRIVATE; echo ":";
echo ReflectionMethod::IS_ABSTRACT; echo ":";
echo ReflectionProperty::IS_STATIC; echo ":";
echo ReflectionProperty::IS_READONLY; echo ":";
echo ReflectionProperty::IS_PUBLIC; echo ":";
echo ReflectionProperty::IS_PROTECTED; echo ":";
echo ReflectionProperty::IS_PRIVATE; echo ":";
echo ReflectionProperty::IS_ABSTRACT; echo ":";
echo ReflectionProperty::IS_PROTECTED_SET; echo ":";
echo ReflectionProperty::IS_PRIVATE_SET; echo ":";
echo ReflectionProperty::IS_VIRTUAL; echo ":";
echo ReflectionProperty::IS_FINAL; echo ":";
echo ReflectionClassConstant::IS_PUBLIC; echo ":";
echo ReflectionClassConstant::IS_PROTECTED; echo ":";
echo ReflectionClassConstant::IS_PRIVATE; echo ":";
echo ReflectionClassConstant::IS_FINAL;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "32:64:65536:16:4:64:16:128:1:2:4:64:2048:4096:512:32:1:2:4:32"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass exposes eval readonly class metadata.
#[test]
fn execute_program_reflects_eval_class_readonly_predicate() {
    let program = parse_fragment(
        br#"class EvalReadonlyPlain {}
readonly class EvalReadonlyReflect {}
final readonly class EvalReadonlyFinalReflect {}
enum EvalReadonlyEnumReflect { case Ready; }
interface EvalReadonlyIface {}
trait EvalReadonlyTrait {}
echo (new ReflectionClass("EvalReadonlyPlain"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyFinalReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyEnumReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyIface"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyTrait"))->isReadOnly() ? "R" : "r";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "rRRrrr");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass exposes eval class instantiability metadata.
#[test]
fn execute_program_reflects_eval_class_instantiable_predicate() {
    let program = parse_fragment(
        br#"abstract class EvalInstAbstract {}
class EvalInstPublic {}
final class EvalInstFinal {}
class EvalInstPrivate { private function __construct() {} }
class EvalInstProtected { protected function __construct() {} }
interface EvalInstIface {}
trait EvalInstTrait {}
enum EvalInstEnum { case Ready; }
echo (new ReflectionClass("EvalInstAbstract"))->isInstantiable() ? "A" : "a";
echo (new ReflectionClass("EvalInstPublic"))->isInstantiable() ? "B" : "b";
echo (new ReflectionClass("EvalInstFinal"))->isInstantiable() ? "C" : "c";
echo (new ReflectionClass("EvalInstPrivate"))->isInstantiable() ? "P" : "p";
echo (new ReflectionClass("EvalInstProtected"))->isInstantiable() ? "R" : "r";
echo (new ReflectionClass("EvalInstIface"))->isInstantiable() ? "I" : "i";
echo (new ReflectionClass("EvalInstTrait"))->isInstantiable() ? "T" : "t";
echo (new ReflectionClass("EvalInstEnum"))->isInstantiable() ? "E" : "e";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "aBCprite");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::isAnonymous reports false for eval-declared named class-like symbols.
#[test]
fn execute_program_reflection_class_reports_named_classes_not_anonymous() {
    let program = parse_fragment(
        br#"class EvalNamedAnonymousReflect {}
interface EvalNamedAnonymousIface {}
trait EvalNamedAnonymousTrait {}
enum EvalNamedAnonymousEnum { case Ready; }
echo (new ReflectionClass("EvalNamedAnonymousReflect"))->isAnonymous() ? "C" : "c";
echo (new ReflectionClass("EvalNamedAnonymousIface"))->isAnonymous() ? "I" : "i";
echo (new ReflectionClass("EvalNamedAnonymousTrait"))->isAnonymous() ? "T" : "t";
echo (new ReflectionClass("EvalNamedAnonymousEnum"))->isAnonymous() ? "E" : "e";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "cite");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::isCloneable reports eval class clone metadata.
#[test]
fn execute_program_reflects_eval_class_cloneable_predicate() {
    let program = parse_fragment(
        br#"abstract class EvalCloneAbstract {}
class EvalClonePlain {}
final class EvalCloneFinal {}
class EvalClonePrivate { private function __clone() {} }
class EvalCloneProtected { protected function __clone() {} }
class EvalClonePublic { public function __clone() {} }
interface EvalCloneIface {}
trait EvalCloneTrait {}
enum EvalCloneEnum { case Ready; }
echo (new ReflectionClass("EvalCloneAbstract"))->isCloneable() ? "A" : "a";
echo (new ReflectionClass("EvalClonePlain"))->isCloneable() ? "P" : "p";
echo (new ReflectionClass("EvalCloneFinal"))->isCloneable() ? "F" : "f";
echo (new ReflectionClass("EvalClonePrivate"))->isCloneable() ? "V" : "v";
echo (new ReflectionClass("EvalCloneProtected"))->isCloneable() ? "R" : "r";
echo (new ReflectionClass("EvalClonePublic"))->isCloneable() ? "U" : "u";
echo (new ReflectionClass("EvalCloneIface"))->isCloneable() ? "I" : "i";
echo (new ReflectionClass("EvalCloneTrait"))->isCloneable() ? "T" : "t";
echo (new ReflectionClass("EvalCloneEnum"))->isCloneable() ? "E" : "e";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "aPFvrUite");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::isIterable reports eval Traversable-compatible class metadata.
#[test]
fn execute_program_reflects_eval_class_iterable_predicate() {
    let program = parse_fragment(
        br#"class EvalIterablePlain {}
abstract class EvalIterableAbstract implements Iterator {}
interface EvalIterableIface extends Iterator {}
trait EvalIterableTrait {}
enum EvalIterableEnum { case Ready; }
class EvalIterableIterator implements Iterator {
    public function current() { return null; }
    public function key() { return null; }
    public function next() {}
    public function valid() { return false; }
    public function rewind() {}
}
class EvalIterableAggregate implements IteratorAggregate {
    public function getIterator() { return $this; }
}
echo (new ReflectionClass("EvalIterablePlain"))->isIterable() ? "P" : "p";
$iter = new ReflectionClass("EvalIterableIterator");
echo $iter->isIterable() ? "I" : "i";
echo $iter->isIterateable() ? "A" : "a";
echo (new ReflectionClass("EvalIterableAggregate"))->isIterable() ? "G" : "g";
echo (new ReflectionClass("EvalIterableAbstract"))->isIterable() ? "B" : "b";
echo (new ReflectionClass("EvalIterableIface"))->isIterable() ? "F" : "f";
echo (new ReflectionClass("EvalIterableEnum"))->isIterable() ? "E" : "e";
echo (new ReflectionClass("EvalIterableTrait"))->isIterable() ? "H" : "h";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "pIAGbfeh");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass origin predicates report eval class-like symbols as user-defined.
#[test]
fn execute_program_reflects_eval_class_origin_predicates() {
    let program = parse_fragment(
        br#"class EvalOriginClass {}
interface EvalOriginIface {}
trait EvalOriginTrait {}
enum EvalOriginEnum { case Ready; }
function eval_reflect_origin($name) {
    $r = new ReflectionClass($name);
    echo $r->isInternal() ? "I" : "i";
    echo $r->isUserDefined() ? "U" : "u";
    echo ":";
}
eval_reflect_origin("EvalOriginClass");
eval_reflect_origin("EvalOriginIface");
eval_reflect_origin("EvalOriginTrait");
eval_reflect_origin("EvalOriginEnum");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "iU:iU:iU:iU:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::getConstructor exposes eval constructor metadata.
#[test]
fn execute_program_reflection_class_get_constructor() {
    let program = parse_fragment(
        br#"class EvalCtorBase {
    public function __construct($required, $optional = 2) {}
}
class EvalCtorChild extends EvalCtorBase {}
class EvalCtorPlain {}
interface EvalCtorInterface {
    public function __construct($required);
}
trait EvalCtorTrait {
    public function __construct($required, $optional = null, ...$rest) {}
}
$base = (new ReflectionClass("EvalCtorBase"))->getConstructor();
echo $base->getName(); echo "/";
echo $base->getNumberOfParameters(); echo "/";
echo $base->getNumberOfRequiredParameters(); echo ":";
$child = (new ReflectionClass("EvalCtorChild"))->getConstructor();
echo $child->getName(); echo "/";
echo $child->getNumberOfParameters(); echo "/";
echo $child->getNumberOfRequiredParameters(); echo ":";
$plain = (new ReflectionClass("EvalCtorPlain"))->getConstructor();
echo $plain === null ? "null" : "bad"; echo ":";
$interface = (new ReflectionClass("EvalCtorInterface"))->getConstructor();
echo $interface->getName(); echo "/";
echo $interface->getNumberOfParameters(); echo "/";
echo $interface->getNumberOfRequiredParameters(); echo ":";
$trait = (new ReflectionClass("EvalCtorTrait"))->getConstructor();
echo $trait->getName(); echo "/";
echo $trait->getNumberOfParameters(); echo "/";
echo $trait->getNumberOfRequiredParameters();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "__construct/2/1:__construct/2/1:null:__construct/1/1:__construct/3/1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass reports eval class-like method, property, and constant membership.
#[test]
fn execute_program_reflects_eval_class_member_existence() {
    let program = parse_fragment(
        br#"class EvalMemberParent {
    const PARENT_CONST = 1;
    private function hiddenParent() {}
    protected static function parentStatic() {}
    private $hiddenProp;
    protected static $parentStaticProp;
}
interface EvalMemberClassIface {
    const CLASS_LIMIT = 10;
}
class EvalMemberChild extends EvalMemberParent implements EvalMemberClassIface {
    const CHILD_CONST = 2;
    public function ChildMethod() {}
    public $childProp;
}
interface EvalMemberIfaceParent {
    const PARENT_LIMIT = 10;
    public function parentRequirement();
}
interface EvalMemberIface extends EvalMemberIfaceParent {
    const CHILD_LIMIT = 20;
    public function childRequirement();
    public string $hook { get; }
}
trait EvalMemberTrait {
    const TRAIT_CONST = 30;
    private function traitHidden() {}
    public $traitProp;
}
enum EvalMemberPureEnum {
    case Ready;
    const LEVEL = 40;
    public function label() { return "ok"; }
}
enum EvalMemberBackedEnum: string {
    case Ready = "ready";
}
$child = new ReflectionClass("EvalMemberChild");
echo $child->hasMethod("childmethod") ? "M" : "m";
echo $child->hasMethod("HIDDENPARENT") ? "P" : "p";
echo $child->hasMethod("parentStatic") ? "S" : "s";
echo $child->hasMethod("missing") ? "X" : "x";
echo ":";
echo $child->hasProperty("childProp") ? "C" : "c";
echo $child->hasProperty("hiddenProp") ? "H" : "h";
echo $child->hasProperty("parentStaticProp") ? "T" : "t";
echo $child->hasProperty("childprop") ? "W" : "w";
echo $child->hasConstant("CHILD_CONST") ? "D" : "d";
echo $child->hasConstant("PARENT_CONST") ? "P" : "p";
echo $child->hasConstant("CLASS_LIMIT") ? "A" : "a";
echo $child->hasConstant("child_const") ? "Z" : "z";
echo ":";
$iface = new ReflectionClass("EvalMemberIface");
echo $iface->hasMethod("parentrequirement") ? "I" : "i";
echo $iface->hasMethod("childRequirement") ? "J" : "j";
echo $iface->hasProperty("hook") ? "K" : "k";
echo $iface->hasConstant("PARENT_LIMIT") ? "L" : "l";
echo $iface->hasConstant("CHILD_LIMIT") ? "C" : "c";
echo ":";
$trait = new ReflectionClass("EvalMemberTrait");
echo $trait->hasMethod("traithidden") ? "R" : "r";
echo $trait->hasProperty("traitProp") ? "U" : "u";
echo $trait->hasConstant("TRAIT_CONST") ? "K" : "k";
echo ":";
$pure = new ReflectionClass("EvalMemberPureEnum");
echo $pure->hasMethod("cases") ? "E" : "e";
echo $pure->hasMethod("label") ? "L" : "l";
echo $pure->hasProperty("name") ? "N" : "n";
echo $pure->hasProperty("value") ? "V" : "v";
echo $pure->hasConstant("Ready") ? "G" : "g";
echo $pure->hasConstant("LEVEL") ? "F" : "f";
echo $pure->hasConstant("ready") ? "R" : "r";
echo ":";
$backed = new ReflectionClass("EvalMemberBackedEnum");
echo $backed->hasMethod("tryfrom") ? "B" : "b";
echo $backed->hasProperty("value") ? "Y" : "y";
echo $backed->hasConstant("Ready") ? "Q" : "q";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "MPSx:ChTwDPAz:IJKLC:RUK:ELNvGFr:BYQ");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass returns eval class-like constant values and enum cases.
#[test]
fn execute_program_reflects_eval_class_constant_values() {
    let program = parse_fragment(
        br#"class EvalReflectConstBase {
    public const BASE = 1;
}
interface EvalReflectConstIface {
    public const LIMIT = 2;
}
trait EvalReflectConstTrait {
    public const TRAIT_VALUE = 3;
}
class EvalReflectConstChild extends EvalReflectConstBase implements EvalReflectConstIface {
    private const SECRET = 9;
    public const OWN = "own";
    public const SUM = 5;
}
enum EvalReflectConstEnum {
    case Ready;
    public const LEVEL = 40;
}
$ref = new ReflectionClass("EvalReflectConstChild");
$all = $ref->getConstants();
$public = $ref->getConstants(ReflectionClassConstant::IS_PUBLIC);
$private = $ref->getConstants(filter: ReflectionClassConstant::IS_PRIVATE);
$none = $ref->getConstants(0);
$null = $ref->getConstants(null);
echo $ref->getConstant("OWN"); echo ":";
echo $ref->getConstant("BASE"); echo ":";
echo $ref->getConstant("LIMIT"); echo ":";
echo $ref->getConstant("SECRET"); echo ":";
echo $ref->getConstant("SUM"); echo ":";
echo $ref->getConstant("own") ? "bad" : "missing";
echo ":"; echo count($all); echo ":"; echo $all["OWN"]; echo ":"; echo $all["BASE"]; echo ":"; echo $all["LIMIT"];
echo ":"; echo count($public); echo ":"; echo $public["OWN"]; echo ":"; echo $public["BASE"];
echo ":"; echo count($private); echo ":"; echo $private["SECRET"];
echo ":"; echo count($none); echo ":"; echo count($null);
$trait = new ReflectionClass("EvalReflectConstTrait");
$traitAll = $trait->getConstants();
echo ":"; echo $trait->getConstant("TRAIT_VALUE"); echo ":"; echo count($traitAll); echo ":"; echo $traitAll["TRAIT_VALUE"];
$enum = new ReflectionClass("EvalReflectConstEnum");
$case = $enum->getConstant("Ready");
$enumAll = $enum->getConstants();
echo ":"; echo $case->name;
echo ":"; echo $enum->getConstant("LEVEL"); echo ":"; echo $enumAll["LEVEL"]; echo ":"; echo count($enumAll);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "own:1:2:9:5:missing:5:own:1:2:4:own:1:1:9:0:5:3:1:3:Ready:40:40:2"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass returns eval class-constant reflector objects.
#[test]
fn execute_program_reflects_eval_class_constant_reflector_objects() {
    let program = parse_fragment(
        br#"class EvalReflectConstMarker {
    public $label;
    public function __construct($label) {
        $this->label = $label;
    }
    public function label() {
        return $this->label;
    }
}
class EvalReflectConstObjectTarget {
    #[EvalReflectConstMarker("const")]
    final public const ANSWER = 42;
}
enum EvalReflectConstObjectEnum {
    #[EvalReflectConstMarker("case")]
    case Ready;
    final public const LEVEL = 7;
}
$ref = new ReflectionClass("EvalReflectConstObjectTarget");
$single = $ref->getReflectionConstant("ANSWER");
$all = $ref->getReflectionConstants();
$public = $ref->getReflectionConstants(ReflectionClassConstant::IS_PUBLIC);
$final = $ref->getReflectionConstants(filter: ReflectionClassConstant::IS_FINAL);
echo $single->getName(); echo ":";
echo count($all); echo ":"; echo $all[0]->getName(); echo ":";
echo $single->getAttributes()[0]->newInstance()->label(); echo ":";
echo $ref->getReflectionConstant("answer") ? "bad" : "missing";
echo ":"; echo count($public); echo ":"; echo $public[0]->getName();
echo ":"; echo count($final); echo ":"; echo $final[0]->getName();
$enum = new ReflectionClass("EvalReflectConstObjectEnum");
$enumAll = $enum->getReflectionConstants();
$enumFinal = $enum->getReflectionConstants(ReflectionClassConstant::IS_FINAL);
$case = $enum->getReflectionConstant("Ready");
$level = $enum->getReflectionConstant("LEVEL");
echo ":"; echo count($enumAll); echo ":"; echo $enumAll[0]->getName(); echo ":"; echo $enumAll[1]->getName();
echo ":"; echo $case->getAttributes()[0]->newInstance()->label(); echo ":";
echo count($level->getAttributes()); echo ":"; echo count($enumFinal); echo ":"; echo $enumFinal[0]->getName();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "ANSWER:1:ANSWER:const:missing:1:ANSWER:1:ANSWER:2:Ready:LEVEL:case:0:1:LEVEL"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod and ReflectionProperty expose eval member predicate metadata.
#[test]
fn execute_program_reflects_eval_member_predicates() {
    let program = parse_fragment(
        br#"abstract class EvalReflectMemberBase {
    protected static function baseStatic() {}
    abstract protected function mustImplement();
    final public function locked() {}
}
readonly class EvalReflectReadonlyClass {
    public int $classReadonly;
}
abstract class EvalReflectAbstractProperty {
    abstract public int $mustRead { get; }
}
class EvalReflectMemberChild extends EvalReflectMemberBase {
    public function mustImplement() {}
    private static $token;
    final public static $staticSeal;
    protected $visible;
    public readonly int $locked;
    final public int $sealed;
}
$baseStatic = new ReflectionMethod("EvalReflectMemberChild", "baseStatic");
echo $baseStatic->isStatic() ? "S" : "s";
echo $baseStatic->isProtected() ? "P" : "p";
echo $baseStatic->isPublic() ? "U" : "u";
echo $baseStatic->isPrivate() ? "R" : "r";
echo $baseStatic->isFinal() ? "F" : "f";
echo $baseStatic->isAbstract() ? "A" : "a";
echo ":";
$abstractMethod = new ReflectionMethod("EvalReflectMemberBase", "mustImplement");
echo $abstractMethod->isAbstract() ? "A" : "a";
echo $abstractMethod->isProtected() ? "P" : "p";
echo $abstractMethod->isStatic() ? "S" : "s";
echo ":";
$finalMethod = new ReflectionMethod("EvalReflectMemberChild", "locked");
echo $finalMethod->isFinal() ? "F" : "f";
echo $finalMethod->isPublic() ? "U" : "u";
echo $finalMethod->isStatic() ? "S" : "s";
echo ":";
$staticProp = new ReflectionProperty("EvalReflectMemberChild", "token");
echo $staticProp->isStatic() ? "S" : "s";
echo $staticProp->isPrivate() ? "R" : "r";
echo $staticProp->isProtected() ? "P" : "p";
echo $staticProp->isFinal() ? "F" : "f";
echo $staticProp->isAbstract() ? "A" : "a";
echo $staticProp->isReadOnly() ? "R" : "r";
echo $staticProp->isProtectedSet() ? "T" : "t";
echo $staticProp->isPrivateSet() ? "D" : "d";
echo $staticProp->getModifiers();
echo ":";
$visibleProp = new ReflectionProperty("EvalReflectMemberChild", "visible");
echo $visibleProp->isStatic() ? "S" : "s";
echo $visibleProp->isProtected() ? "P" : "p";
echo $visibleProp->isPublic() ? "U" : "u";
echo $visibleProp->isFinal() ? "F" : "f";
echo $visibleProp->isAbstract() ? "A" : "a";
echo $visibleProp->isReadOnly() ? "R" : "r";
echo $visibleProp->isProtectedSet() ? "T" : "t";
echo $visibleProp->isPrivateSet() ? "D" : "d";
echo $visibleProp->getModifiers();
echo ":";
$readonlyProp = new ReflectionProperty("EvalReflectMemberChild", "locked");
echo $readonlyProp->isReadOnly() ? "R" : "r";
echo $readonlyProp->isPublic() ? "U" : "u";
echo $readonlyProp->isProtectedSet() ? "T" : "t";
echo $readonlyProp->isPrivateSet() ? "D" : "d";
echo $readonlyProp->getModifiers();
echo ":";
$sealedProp = new ReflectionProperty("EvalReflectMemberChild", "sealed");
echo $sealedProp->isFinal() ? "F" : "f";
echo $sealedProp->isPublic() ? "U" : "u";
echo $sealedProp->getModifiers();
echo ":";
$staticFinalProp = new ReflectionProperty("EvalReflectMemberChild", "staticSeal");
echo $staticFinalProp->isFinal() ? "F" : "f";
echo $staticFinalProp->isStatic() ? "S" : "s";
echo $staticFinalProp->getModifiers();
echo ":";
$abstractProp = new ReflectionProperty("EvalReflectAbstractProperty", "mustRead");
echo $abstractProp->isAbstract() ? "A" : "a";
echo $abstractProp->isFinal() ? "F" : "f";
echo $abstractProp->getModifiers();
echo ":";
$classReadonlyProp = new ReflectionProperty("EvalReflectReadonlyClass", "classReadonly");
echo $classReadonlyProp->isReadOnly() ? "C" : "c";
echo $classReadonlyProp->isProtectedSet() ? "T" : "t";
echo $classReadonlyProp->isPrivateSet() ? "D" : "d";
echo $classReadonlyProp->getModifiers();
echo ":";
echo $visibleProp->isDynamic() ? "D" : "d";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "SPurfa:APs:FUs:SRpfartd20:sPufartd2:RUTd2177:FU33:FS49:Af577:CTd2177:d"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionProperty reports eval-declared asymmetric set visibility.
#[test]
fn execute_program_reflects_eval_asymmetric_property_set_visibility() {
    let program = parse_fragment(
        br#"class EvalReflectAsymmetricProperty {
    public private(set) int $privateSet = 1;
    public protected(set) int $protectedSet = 2;
    protected private(set) int $protectedPrivateSet = 3;
}
$private = new ReflectionProperty("EvalReflectAsymmetricProperty", "privateSet");
echo $private->isPrivateSet() ? "P" : "p";
echo $private->isProtectedSet() ? "T" : "t";
echo $private->getModifiers(); echo ":";
$protected = new ReflectionProperty("EvalReflectAsymmetricProperty", "protectedSet");
echo $protected->isPrivateSet() ? "P" : "p";
echo $protected->isProtectedSet() ? "T" : "t";
echo $protected->getModifiers(); echo ":";
$protectedPrivate = new ReflectionProperty("EvalReflectAsymmetricProperty", "protectedPrivateSet");
echo $protectedPrivate->isPrivateSet() ? "P" : "p";
echo $protectedPrivate->isProtectedSet() ? "T" : "t";
echo $protectedPrivate->getModifiers();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Pt4129:pT2049:Pt4130");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionProperty reports asymmetric set visibility on eval interface contracts.
#[test]
fn execute_program_reflects_eval_interface_asymmetric_property_set_visibility() {
    let program = parse_fragment(
        br#"interface EvalReflectAsymmetricIfaceProperty {
    public protected(set) string $name { get; set; }
    private(set) int $id { get; set; }
}
$protected = new ReflectionProperty("EvalReflectAsymmetricIfaceProperty", "name");
echo $protected->isProtectedSet() ? "T" : "t";
echo $protected->isPrivateSet() ? "P" : "p";
echo $protected->isFinal() ? "F" : "f";
echo $protected->getModifiers(); echo ":";
$private = new ReflectionProperty("EvalReflectAsymmetricIfaceProperty", "id");
echo $private->isProtectedSet() ? "T" : "t";
echo $private->isPrivateSet() ? "P" : "p";
echo $private->isFinal() ? "F" : "f";
echo $private->getModifiers();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Tpf2625:tPF4705");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionProperty reports eval constructor-promotion metadata.
#[test]
fn execute_program_reflection_property_reports_eval_promoted_metadata() {
    let program = parse_fragment(
        br#"class EvalReflectPromotedTarget {
    public function __construct(public int $id, private string $name = "Ada") {}
    public string $plain = "x";
}
$id = new ReflectionProperty("EvalReflectPromotedTarget", "id");
$name = new ReflectionProperty("EvalReflectPromotedTarget", "name");
$plain = new ReflectionProperty("EvalReflectPromotedTarget", "plain");
echo $id->isPromoted() ? "I" : "i";
echo $name->isPromoted() ? "N" : "n";
echo $plain->isPromoted() ? "P" : "p";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "INp");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod constructor/destructor predicates for eval methods.
#[test]
fn execute_program_reflection_method_reports_constructor_and_destructor() {
    let program = parse_fragment(
        br#"class EvalReflectLifecycle {
    public function __construct() {}
    public function __destruct() {}
    public function run() {}
}
$ctor = new ReflectionMethod("EvalReflectLifecycle", "__CONSTRUCT");
echo $ctor->isConstructor() ? "C" : "c";
echo $ctor->isDestructor() ? "D" : "d";
echo ":";
$dtor = new ReflectionMethod("EvalReflectLifecycle", "__destruct");
echo $dtor->isConstructor() ? "C" : "c";
echo $dtor->isDestructor() ? "D" : "d";
echo ":";
$run = new ReflectionMethod("EvalReflectLifecycle", "run");
echo $run->isConstructor() ? "C" : "c";
echo $run->isDestructor() ? "D" : "d";
echo ":";
$listed = (new ReflectionClass("EvalReflectLifecycle"))->getConstructor();
echo $listed->isConstructor() ? "C" : "c";
echo $listed->isDestructor() ? "D" : "d";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Cd:cD:cd:Cd");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod preserves declared method case after case-insensitive lookup.
#[test]
fn execute_program_reflection_method_preserves_declared_name_case() {
    let program = parse_fragment(
        br#"class EvalReflectMethodCaseBase {
    public function MiXeDCase() { return "base"; }
}
class EvalReflectMethodCaseChild extends EvalReflectMethodCaseBase {
    public function childCase() { return "child"; }
}
$object = new EvalReflectMethodCaseChild();
$direct = new ReflectionMethod("EvalReflectMethodCaseChild", "mixedcase");
echo $direct->getName(); echo ":";
echo $direct->getShortName(); echo ":";
echo $direct->invoke($object); echo ":";
$listed = (new ReflectionClass("EvalReflectMethodCaseChild"))->getMethod("CHILDCASE");
echo $listed->getName(); echo ":";
echo $listed->invoke($object);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "MiXeDCase:MiXeDCase:base:childCase:child");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod accepts object targets and reflects the runtime class.
#[test]
fn execute_program_reflection_method_accepts_object_targets() {
    let program = parse_fragment(
        br#"class EvalReflectMethodObjectBase {
    public function MiXeDCase() { return "base"; }
}
class EvalReflectMethodObjectChild extends EvalReflectMethodObjectBase {
    public function childCase() { return "child"; }
}
$object = new EvalReflectMethodObjectChild();
$inherited = new ReflectionMethod($object, "mixedcase");
echo $inherited->getName(); echo ":";
echo $inherited->getDeclaringClass()->getName(); echo ":";
echo $inherited->invoke($object); echo ":";
$own = new ReflectionMethod($object, "CHILDCASE");
echo $own->getName(); echo ":";
echo $own->getDeclaringClass()->getName(); echo ":";
echo $own->invoke($object);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "MiXeDCase:EvalReflectMethodObjectBase:base:childCase:EvalReflectMethodObjectChild:child"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval member and enum-case reflectors expose their declaring class.
#[test]
fn execute_program_reflects_eval_declaring_class_metadata() {
    let program = parse_fragment(
        br#"class EvalDeclaringBase {
    public $baseProp = 1;
    public function inherited() { return "base"; }
    public const BASE_CONST = 10;
}
class EvalDeclaringChild extends EvalDeclaringBase {
    public $childProp = 2;
    public function own() { return "child"; }
    public const CHILD_CONST = 20;
}
enum EvalDeclaringEnum: string {
    case Ready = "ready";
    public const LEVEL = 3;
}
echo (new ReflectionMethod("EvalDeclaringChild", "inherited"))->getDeclaringClass()->getName(); echo ":";
echo (new ReflectionClass("EvalDeclaringChild"))->getMethod("own")->getDeclaringClass()->getName(); echo ":";
echo (new ReflectionProperty("EvalDeclaringChild", "baseProp"))->getDeclaringClass()->getName(); echo ":";
echo (new ReflectionClass("EvalDeclaringChild"))->getProperty("childProp")->getDeclaringClass()->getName(); echo ":";
echo (new ReflectionClass("EvalDeclaringChild"))->getReflectionConstant("BASE_CONST")->getDeclaringClass()->getName(); echo ":";
echo (new ReflectionClassConstant("EvalDeclaringChild", "BASE_CONST"))->getDeclaringClass()->getName(); echo ":";
echo (new ReflectionClass("EvalDeclaringEnum"))->getReflectionConstant("Ready")->getDeclaringClass()->getName(); echo ":";
echo (new ReflectionEnumBackedCase("EvalDeclaringEnum", "Ready"))->getDeclaringClass()->getName();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "EvalDeclaringBase:EvalDeclaringChild:EvalDeclaringBase:EvalDeclaringChild:EvalDeclaringBase:EvalDeclaringBase:EvalDeclaringEnum:EvalDeclaringEnum"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

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

/// Verifies ReflectionParameter reports eval constant-default metadata.
#[test]
fn execute_program_reflection_parameter_reports_default_constant_metadata() {
    let program = parse_fragment(
        br##"define("EVAL_REFLECT_PARAM_DEFAULT_GLOBAL", "G");
class EvalReflectParamDefaultBase {
    const BASE = "B";
}
class EvalReflectParamDefaultTarget extends EvalReflectParamDefaultBase {
    const LABEL = "L";
    public function run($required, $global = EVAL_REFLECT_PARAM_DEFAULT_GLOBAL, $self = self::LABEL, $parent = parent::BASE, $literal = 7) {}
}
$params = (new ReflectionMethod("EvalReflectParamDefaultTarget", "run"))->getParameters();
foreach ($params as $param) {
    echo $param->getName(); echo ":";
    echo $param->isDefaultValueAvailable() ? "D:" : "d:";
    if ($param->isDefaultValueAvailable()) {
        if ($param->isDefaultValueConstant()) {
            echo "C:";
            echo $param->getDefaultValueConstantName();
            echo ":";
        } else {
            echo "c:null:";
        }
        echo $param->getDefaultValue();
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
        "required:d:|global:D:C:EVAL_REFLECT_PARAM_DEFAULT_GLOBAL:G|self:D:C:self::LABEL:L|parent:D:C:parent::BASE:B|literal:D:c:null:7|"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass exposes eval property default metadata as an associative map.
#[test]
fn execute_program_reflection_class_get_default_properties_metadata() {
    let program = parse_fragment(
        br#"class EvalReflectDefaultBase {
    public int $base = 1;
    protected string $prot = "p";
    private int $shadow = 3;
    public $implicit;
    public int $typed;
    public static string $baseStatic = "bs";
}
class EvalReflectDefaultChild extends EvalReflectDefaultBase {
    public int $child = 5;
    private int $shadow = 9;
    public static int $childStatic = 7;
    public ?int $nullable = null;
}
$defaults = (new ReflectionClass("EvalReflectDefaultChild"))->getDefaultProperties();
echo $defaults["childStatic"]; echo ":";
echo $defaults["baseStatic"]; echo ":";
echo $defaults["child"]; echo ":";
echo $defaults["shadow"]; echo ":";
echo $defaults["base"]; echo ":";
echo $defaults["prot"]; echo ":";
echo array_key_exists("implicit", $defaults) && $defaults["implicit"] === null ? "I:" : "i:";
echo array_key_exists("nullable", $defaults) && $defaults["nullable"] === null ? "N:" : "n:";
echo array_key_exists("typed", $defaults) ? "T" : "t";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "7:bs:5:9:1:p:I:N:t");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

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

/// Verifies ReflectionClassConstant/EnumCase expose eval-declared attribute metadata.
#[test]
fn execute_program_reflects_eval_constant_and_enum_case_attributes() {
    let program = parse_fragment(
        br#"class EvalConstMarker {
    public $name;
    public function __construct($name) {
        $this->name = $name;
    }
    public function label() {
        return $this->name;
    }
}
class EvalConstReflectTarget {
    #[EvalConstMarker("const")]
    final public const ANSWER = 42;
}
enum EvalCaseReflectTarget: string {
    #[EvalConstMarker("case")]
    case Ready = "ready";
}
$const_attrs = (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->getAttributes();
echo count($const_attrs); echo ":"; echo (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->getName(); echo ":";
echo (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->isFinal() ? "F" : "f"; echo ":";
echo (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->isEnumCase() ? "enum" : "plain"; echo ":";
echo (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->getValue(); echo ":";
echo ((new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->getValue() === EvalCaseReflectTarget::Ready) ? "E" : "e"; echo ":";
echo (new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->isEnumCase() ? "enum" : "plain"; echo ":";
echo $const_attrs[0]->getName(); echo ":"; echo $const_attrs[0]->getArguments()[0]; echo ":";
echo $const_attrs[0]->newInstance()->label(); echo ":";
$case_attrs = (new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo count($case_attrs); echo ":"; echo (new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->getName(); echo ":";
echo $case_attrs[0]->getName(); echo ":"; echo $case_attrs[0]->getArguments()[0]; echo ":";
$unit_attrs = (new ReflectionEnumUnitCase("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo (new ReflectionEnumUnitCase("EvalCaseReflectTarget", "Ready"))->getName(); echo ":";
echo ((new ReflectionEnumUnitCase("EvalCaseReflectTarget", "Ready"))->getValue() === EvalCaseReflectTarget::Ready) ? "unit" : "bad"; echo ":";
echo $unit_attrs[0]->newInstance()->label(); echo ":";
$backed_attrs = (new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo (new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getName(); echo ":";
echo ((new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getValue() === EvalCaseReflectTarget::Ready) ? "backed" : "bad"; echo ":";
echo (new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getBackingValue(); echo ":";
echo $backed_attrs[0]->newInstance()->label();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:ANSWER:F:plain:42:E:enum:EvalConstMarker:const:const:1:Ready:EvalConstMarker:case:Ready:unit:case:Ready:backed:ready:case"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies unsupported attribute argument metadata remains name-visible but not materializable.
#[test]
fn execute_program_rejects_unsupported_class_attribute_args_metadata() {
    let program = parse_fragment(
        br#"#[Tag($dynamic)]
class EvalUnsupportedAttr {}
$names = class_attribute_names("EvalUnsupportedAttr");
echo count($names); echo ":"; echo $names[0]; echo ":";
class_attribute_args("EvalUnsupportedAttr", "Tag");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("unsupported attribute metadata should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
    assert_eq!(values.output, "1:Tag:");
}
