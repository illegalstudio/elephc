//! Purpose:
//! Interpreter tests for reflected member discovery, flags, and declaring metadata.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Methods, properties, class constants, and enum cases share this metadata layer.

use super::super::super::*;
use super::super::support::*;

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
