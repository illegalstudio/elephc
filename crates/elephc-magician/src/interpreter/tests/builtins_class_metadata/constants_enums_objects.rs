//! Purpose:
//! Interpreter tests for class constants, enum cases, and ReflectionObject.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Enum ownership, constant types, dynamic properties, and constructor errors are covered.

use super::super::super::*;
use super::super::support::*;

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

/// Verifies ReflectionClassConstant and enum case metadata expose PHP's untyped defaults.
#[test]
fn execute_program_reflects_eval_constant_type_metadata_defaults() {
    let program = parse_fragment(
        br#"class EvalConstTypeTarget {
    public const ANSWER = 42;
}
enum EvalConstTypeEnum: string {
    case Ready = "ready";
}
$constant = new ReflectionClassConstant("EvalConstTypeTarget", "ANSWER");
echo $constant->isDeprecated() ? "D" : "d"; echo ":";
echo $constant->hasType() ? "T" : "t"; echo ":";
echo $constant->getType() === null ? "N" : "n"; echo ":";
$case = new ReflectionClassConstant("EvalConstTypeEnum", "Ready");
echo $case->isDeprecated() ? "D" : "d"; echo ":";
echo $case->hasType() ? "T" : "t"; echo ":";
echo $case->getType() === null ? "N" : "n"; echo ":";
$unit = new ReflectionEnumUnitCase("EvalConstTypeEnum", "Ready");
echo $unit->isDeprecated() ? "D" : "d"; echo ":";
echo $unit->hasType() ? "T" : "t"; echo ":";
echo $unit->getType() === null ? "N" : "n"; echo ":";
$backed = new ReflectionEnumBackedCase("EvalConstTypeEnum", "Ready");
echo $backed->isDeprecated() ? "D" : "d"; echo ":";
echo $backed->hasType() ? "T" : "t"; echo ":";
echo $backed->getType() === null ? "N" : "n";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "d:t:N:d:t:N:d:t:N:d:t:N");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClassConstant and enum case objects stringify retained metadata.
#[test]
fn execute_program_reflects_eval_constant_to_string() {
    let program = parse_fragment(
        br#"class EvalConstStringTarget {
    public const ANSWER = 42;
    final protected const LIMIT = 7;
    private const FLAG = true;
    public const LABEL = "ok";
    public const NOTHING = null;
}
enum EvalConstStringEnum: string {
    case Ready = "ready";
}
foreach (["ANSWER", "LIMIT", "FLAG", "LABEL", "NOTHING"] as $name) {
    echo str_replace("\n", "\\n", (new ReflectionClassConstant("EvalConstStringTarget", $name))->__toString());
    echo "|";
}
echo str_replace("\n", "\\n", (new ReflectionClassConstant("EvalConstStringEnum", "Ready"))->__toString());
echo "|";
echo str_replace("\n", "\\n", (new ReflectionEnumUnitCase("EvalConstStringEnum", "Ready"))->__toString());
echo "|";
echo str_replace("\n", "\\n", (new ReflectionEnumBackedCase("EvalConstStringEnum", "Ready"))->__toString());
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Constant [ public int ANSWER ] { 42 }\\n|Constant [ final protected int LIMIT ] { 7 }\\n|Constant [ private bool FLAG ] { 1 }\\n|Constant [ public string LABEL ] { ok }\\n|Constant [ public null NOTHING ] {  }\\n|Constant [ public EvalConstStringEnum Ready ] { Object }\\n|Constant [ public EvalConstStringEnum Ready ] { Object }\\n|Constant [ public EvalConstStringEnum Ready ] { Object }\\n"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies enum-case reflection owners expose inherited constant metadata predicates.
#[test]
fn execute_program_reflects_enum_case_visibility_and_modifiers() {
    let program = parse_fragment(
        br#"enum EvalEnumCaseVisibility: string {
    case Ready = "ready";
}
$unit = new ReflectionEnumUnitCase("EvalEnumCaseVisibility", "Ready");
$backed = new ReflectionEnumBackedCase("EvalEnumCaseVisibility", "Ready");
foreach ([$unit, $backed] as $case) {
    echo $case->isEnumCase() ? "E" : "e";
    echo $case->isPrivate() ? "R" : "r";
    echo $case->isProtected() ? "P" : "p";
    echo $case->isPublic() ? "U" : "u";
    echo $case->isFinal() ? "F" : "f";
    echo $case->getModifiers(); echo ":";
}
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

    assert_eq!(values.output, "ErpUf1:ErpUf1:1:2:4:32:1:2:4:32");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionEnum exposes eval-declared enum cases and backing metadata.
#[test]
fn execute_program_reflects_eval_enum_owner_metadata() {
    let program = parse_fragment(
        br#"enum EvalReflectPure {
    case Ready;
    case Done;
}
enum EvalReflectBacked: string {
    case Ready = "ready";
    case Done = "done";
}
$pure = new ReflectionEnum("EvalReflectPure");
echo $pure->getName(); echo ":";
echo $pure->isEnum() ? "E" : "e"; echo ":";
echo $pure->isBacked() ? "B" : "b"; echo ":";
echo $pure->getBackingType() === null ? "N" : "n"; echo ":";
echo $pure->hasCase("Ready") ? "R" : "r";
echo $pure->hasCase("Missing") ? "M" : "m"; echo ":";
$case = $pure->getCase("Done");
echo $case->getName(); echo ":";
echo $case->getEnum()->getName(); echo ":";
$cases = $pure->getCases();
echo count($cases); echo ":";
echo $cases[0]->getName(); echo ":";
echo $cases[1]->getEnum()->getName(); echo ":";
$backed = new ReflectionEnum("EvalReflectBacked");
$type = $backed->getBackingType();
echo $backed->isBacked() ? "B" : "b"; echo ":";
echo $type->getName(); echo ":";
echo $type->isBuiltin() ? "I" : "i"; echo ":";
$backed_case = $backed->getCase("Ready");
echo $backed_case->getName(); echo ":";
echo $backed_case->getBackingValue(); echo ":";
echo $backed_case->getEnum()->isBacked() ? "E" : "e"; echo ":";
$backed_cases = $backed->getCases();
echo count($backed_cases); echo ":";
echo $backed_cases[1]->getBackingValue(); echo ":";
echo $backed_cases[0]->getEnum()->getBackingType()->getName();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "EvalReflectPure:E:b:N:Rm:Done:EvalReflectPure:2:Ready:EvalReflectPure:B:string:I:Ready:ready:E:2:done:string"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionEnum construction throws catchable PHP reflection errors.
#[test]
fn execute_program_reflection_enum_constructor_throws_reflection_exceptions() {
    let program = parse_fragment(
        br#"class EvalReflectNotEnumClass {}
interface EvalReflectNotEnumIface {}
trait EvalReflectNotEnumTrait {}
enum EvalReflectActualEnum {
    case Ready;
}
try {
    new ReflectionEnum("EvalReflectNotEnumClass");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnum("EvalReflectNotEnumIface");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnum("EvalReflectNotEnumTrait");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnum("EvalReflectMissingEnum");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
echo (new ReflectionEnum("EvalReflectActualEnum"))->getName();
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
        "ReflectionException:Class \"EvalReflectNotEnumClass\" is not an enum|Class \"EvalReflectNotEnumIface\" is not an enum|Class \"EvalReflectNotEnumTrait\" is not an enum|Class \"EvalReflectMissingEnum\" does not exist|EvalReflectActualEnum"
    );
    assert_eq!(values.get(result.expect("execute eval ir")), FakeValue::Bool(true));
}

/// Verifies ReflectionObject reflects the runtime class of an eval object instance.
#[test]
fn execute_program_reflection_object_reflects_eval_instances() {
    let program = parse_fragment(
        br#"class EvalReflectObjectBase {
    public function inherited(): string {
        return "base";
    }
}
class EvalReflectObjectChild extends EvalReflectObjectBase {
    public int $count = 3;
}
$ref = new ReflectionObject(new EvalReflectObjectChild());
echo get_class($ref); echo ":";
echo ($ref instanceof ReflectionObject) ? "O" : "o";
echo ($ref instanceof ReflectionClass) ? "C" : "c"; echo ":";
echo $ref->getName(); echo ":";
echo $ref->getParentClass()->getName(); echo ":";
echo $ref->hasMethod("inherited") ? "M" : "m";
echo $ref->hasProperty("count") ? "P" : "p"; echo ":";
$object = $ref->newInstanceWithoutConstructor();
echo get_class($object);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "ReflectionObject:OC:EvalReflectObjectChild:EvalReflectObjectBase:MP:EvalReflectObjectChild"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionObject lists dynamic public properties from the reflected instance.
#[test]
fn execute_program_reflection_object_lists_dynamic_properties() {
    let program = parse_fragment(
        br#"class EvalReflectObjectDynamicTarget {
    public $declared = "declared";
}
$object = new EvalReflectObjectDynamicTarget();
$object->dynamic = "value";
$ref = new ReflectionObject($object);
$properties = $ref->getProperties();
foreach ($properties as $property) {
    echo $property->getName(); echo ":";
    echo $property->isDynamic() ? "D" : "d"; echo "|";
}
echo ":";
$dynamic = $ref->getProperty("dynamic");
echo $dynamic->isDynamic() ? "D" : "d"; echo ":";
echo $dynamic->getValue($object); echo ":";
echo count($ref->getProperties(ReflectionProperty::IS_PUBLIC)); echo ":";
echo count($ref->getProperties(ReflectionProperty::IS_STATIC)); echo ":";
echo $ref->hasProperty("dynamic") ? "H" : "h";
echo $ref->hasProperty("declared") ? "D" : "d";
echo $ref->hasProperty("missing") ? "M" : "m";
echo (new ReflectionClass($object))->hasProperty("dynamic") ? "C" : "c";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "declared:d|dynamic:D|:D:value:2:0:HDmc");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionObject constructor type errors are catchable.
#[test]
fn execute_program_reflection_object_constructor_throws_type_errors() {
    let program = parse_fragment(
        br#"try {
    new ReflectionObject("EvalReflectObjectChild");
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    new ReflectionObject([]);
    echo "bad";
} catch (TypeError $e) {
    echo $e->getMessage();
}
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
        "TypeError:ReflectionObject::__construct(): Argument #1 ($object) must be of type object, string given|ReflectionObject::__construct(): Argument #1 ($object) must be of type object, array given"
    );
    assert_eq!(values.get(result.expect("execute eval ir")), FakeValue::Bool(true));
}

/// Verifies unsupported attribute argument metadata remains name-visible but not materializable.
#[test]
fn execute_program_rejects_unsupported_class_attribute_args_metadata() {
    for source in [
        br#"#[Tag($dynamic)]
class EvalUnsupportedAttr {}
$names = class_attribute_names("EvalUnsupportedAttr");
echo count($names); echo ":"; echo $names[0]; echo ":";
class_attribute_args("EvalUnsupportedAttr", "Tag");"# as &[u8],
        br#"#[Tag(["fixed" => "ok", $dynamic => "bad"])]
class EvalUnsupportedAttr {}
$names = class_attribute_names("EvalUnsupportedAttr");
echo count($names); echo ":"; echo $names[0]; echo ":";
class_attribute_args("EvalUnsupportedAttr", "Tag");"#,
    ] {
        let program = parse_fragment(source).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let err = execute_program(&program, &mut scope, &mut values)
            .expect_err("unsupported attribute metadata should fail");

        assert_eq!(err, EvalStatus::RuntimeFatal);
        assert_eq!(values.output, "1:Tag:");
    }
}
