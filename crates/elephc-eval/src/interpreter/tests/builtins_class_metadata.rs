//! Purpose:
//! Interpreter tests for eval class metadata and relation builtins.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
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
trait EvalRelationTrait {}
class EvalRelationTarget implements EvalRelationIface {
    use EvalRelationTrait;
}
interface EvalRelationParent {}
interface EvalRelationChild extends EvalRelationParent {}
$ref = new ReflectionClass("EvalRelationTarget");
$interfaces = $ref->getInterfaceNames();
$traits = $ref->getTraitNames();
echo count($interfaces); echo ":"; echo $interfaces[0]; echo ":";
echo count($traits); echo ":"; echo $traits[0]; echo ":";
$parentInterfaces = (new ReflectionClass("EvalRelationChild"))->getInterfaceNames();
echo count($parentInterfaces); echo ":"; echo $parentInterfaces[0];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:EvalRelationIface:1:EvalRelationTrait:1:EvalRelationParent"
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
echo $ref->getConstant("OWN"); echo ":";
echo $ref->getConstant("BASE"); echo ":";
echo $ref->getConstant("LIMIT"); echo ":";
echo $ref->getConstant("SECRET"); echo ":";
echo $ref->getConstant("SUM"); echo ":";
echo $ref->getConstant("own") ? "bad" : "missing";
echo ":"; echo count($all); echo ":"; echo $all["OWN"]; echo ":"; echo $all["BASE"]; echo ":"; echo $all["LIMIT"];
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
        "own:1:2:9:5:missing:5:own:1:2:3:1:3:Ready:40:40:2"
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
    public const ANSWER = 42;
}
enum EvalReflectConstObjectEnum {
    #[EvalReflectConstMarker("case")]
    case Ready;
    public const LEVEL = 7;
}
$ref = new ReflectionClass("EvalReflectConstObjectTarget");
$single = $ref->getReflectionConstant("ANSWER");
$all = $ref->getReflectionConstants();
echo $single->getName(); echo ":";
echo count($all); echo ":"; echo $all[0]->getName(); echo ":";
echo $single->getAttributes()[0]->newInstance()->label(); echo ":";
echo $ref->getReflectionConstant("answer") ? "bad" : "missing";
$enum = new ReflectionClass("EvalReflectConstObjectEnum");
$enumAll = $enum->getReflectionConstants();
$case = $enum->getReflectionConstant("Ready");
$level = $enum->getReflectionConstant("LEVEL");
echo ":"; echo count($enumAll); echo ":"; echo $enumAll[0]->getName(); echo ":"; echo $enumAll[1]->getName();
echo ":"; echo $case->getAttributes()[0]->newInstance()->label(); echo ":";
echo count($level->getAttributes());
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "ANSWER:1:ANSWER:const:missing:2:Ready:LEVEL:case:0"
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
class EvalReflectMemberChild extends EvalReflectMemberBase {
    public function mustImplement() {}
    private static $token;
    protected $visible;
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
echo ":";
$visibleProp = new ReflectionProperty("EvalReflectMemberChild", "visible");
echo $visibleProp->isStatic() ? "S" : "s";
echo $visibleProp->isProtected() ? "P" : "p";
echo $visibleProp->isPublic() ? "U" : "u";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "SPurfa:APs:FUs:SRp:sPu");
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
    public function run(#[EvalParamTag("first")] int &$first, int|string $union, #[EvalParamTag("both")] EvalReflectLeft&EvalReflectRight $both, \App\Name|null $second = null, &...$rest) {}
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
    echo $param->hasType() ? "T" : "t";
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
        "5/3:first#0rvRT:int!B:A1:EvalParamTag:first:d|union#1rvbT:union!:intB:stringB:A0:d|both#2rvbT:intersection!:EvalReflectLeftC:EvalReflectRightC:A1:EvalParamTag:both:d|second#3OvbT:App\\Name?C:A0:D=null|rest#4OVRt:null:A0:d|"
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
echo count($methods); echo ":"; echo count($properties); echo ":";
foreach ($methods as $method) {
    if ($method->getName() === "first") {
        echo "F"; echo count($method->getAttributes());
    }
    if ($method->getName() === "helper") {
        echo $method->isStatic() ? "S" : "s";
        echo $method->isPrivate() ? "R" : "r";
    }
}
echo ":";
foreach ($properties as $property) {
    if ($property->getName() === "visible") {
        echo "V"; echo count($property->getAttributes());
        echo $property->isProtected() ? "P" : "p";
    }
    if ($property->getName() === "token") {
        echo $property->isStatic() ? "T" : "t";
        echo $property->isPrivate() ? "R" : "r";
    }
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:2:F1SR:V1PTR");
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
echo $second->label();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "IJ:KL");
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
    public const ANSWER = 42;
}
enum EvalCaseReflectTarget: string {
    #[EvalConstMarker("case")]
    case Ready = "ready";
}
$const_attrs = (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->getAttributes();
echo count($const_attrs); echo ":"; echo (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->getName(); echo ":";
echo $const_attrs[0]->getName(); echo ":"; echo $const_attrs[0]->getArguments()[0]; echo ":";
echo $const_attrs[0]->newInstance()->label(); echo ":";
$case_attrs = (new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo count($case_attrs); echo ":"; echo (new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->getName(); echo ":";
echo $case_attrs[0]->getName(); echo ":"; echo $case_attrs[0]->getArguments()[0]; echo ":";
$unit_attrs = (new ReflectionEnumUnitCase("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo (new ReflectionEnumUnitCase("EvalCaseReflectTarget", "Ready"))->getName(); echo ":";
echo $unit_attrs[0]->newInstance()->label(); echo ":";
$backed_attrs = (new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo (new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getName(); echo ":";
echo $backed_attrs[0]->newInstance()->label();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:ANSWER:EvalConstMarker:const:const:1:Ready:EvalConstMarker:case:Ready:case:Ready:case"
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
