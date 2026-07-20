//! Purpose:
//! Interpreter tests for ReflectionClass identity, relations, and modifier flags.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Eval and AOT class-like targets are checked through the same APIs.

use super::super::super::*;
use super::super::support::*;

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

/// Verifies ReflectionClass relation-name helpers read fake generated/AOT metadata.
#[test]
fn execute_program_reflects_aot_class_relation_names() {
    let program = parse_fragment(
        br#"$class_names = (new ReflectionClass("KnownClass"))->getInterfaceNames();
echo count($class_names); echo ":"; echo $class_names[0]; echo ":";
$interface_names = (new ReflectionClass("KnownInterface"))->getInterfaceNames();
echo count($interface_names); echo ":"; echo $interface_names[0]; echo ":";
$class_objects = (new ReflectionClass("KnownClass"))->getInterfaces();
echo count($class_objects); echo ":"; echo $class_objects["KnownInterface"]->getName(); echo ":";
$interface_objects = (new ReflectionClass("KnownInterface"))->getInterfaces();
echo count($interface_objects); echo ":"; echo $interface_objects["Traversable"]->getName(); echo ":";
$trait_names = (new ReflectionClass("KnownClass"))->getTraitNames();
echo count($trait_names); echo ":"; echo $trait_names[0]; echo ":";
$trait_objects = (new ReflectionClass("KnownClass"))->getTraits();
echo count($trait_objects); echo ":"; echo $trait_objects["KnownTrait"]->getName(); echo ":";
$nested_trait_names = (new ReflectionClass("KnownTrait"))->getTraitNames();
echo count($nested_trait_names); echo ":"; echo $nested_trait_names[0]; echo ":";
$aliases = (new ReflectionClass("KnownClass"))->getTraitAliases();
echo count($aliases); echo ":"; echo $aliases["knownAlias"];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:KnownInterface:1:Traversable:1:KnownInterface:1:Traversable:1:KnownTrait:1:KnownTrait:1:KnownInnerTrait:1:KnownTrait::source"
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
echo ReflectionClassConstant::IS_FINAL; echo ":";
echo ReflectionEnumUnitCase::IS_PUBLIC; echo ":";
echo ReflectionEnumUnitCase::IS_PROTECTED; echo ":";
echo ReflectionEnumUnitCase::IS_PRIVATE; echo ":";
echo ReflectionEnumUnitCase::IS_FINAL; echo ":";
echo ReflectionEnumBackedCase::IS_PUBLIC; echo ":";
echo ReflectionEnumBackedCase::IS_PROTECTED; echo ":";
echo ReflectionEnumBackedCase::IS_PRIVATE; echo ":";
echo ReflectionEnumBackedCase::IS_FINAL;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "32:64:65536:16:4:64:16:128:1:2:4:64:2048:4096:512:32:1:2:4:32:1:2:4:32:1:2:4:32"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
