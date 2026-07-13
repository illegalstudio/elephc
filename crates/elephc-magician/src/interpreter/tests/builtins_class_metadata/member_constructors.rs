//! Purpose:
//! Interpreter tests for Reflection member constructors and constructor failures.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Constructor target normalization and PHP-visible exceptions are asserted together.

use super::super::super::*;
use super::super::support::*;

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

/// Verifies ReflectionMethod::createFromMethodName resolves eval method strings.
#[test]
fn execute_program_reflection_method_create_from_method_name() {
    let program = parse_fragment(
        br#"class EvalReflectCreateMethodTarget {
    public function MiXeDCase() { return "ok"; }
}
$ref = ReflectionMethod::createFromMethodName("EvalReflectCreateMethodTarget::mixedcase");
echo $ref->getDeclaringClass()->getName(); echo ":";
echo $ref->getName(); echo ":";
echo $ref->invoke(new EvalReflectCreateMethodTarget());
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "EvalReflectCreateMethodTarget:MiXeDCase:ok");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod accepts PHP's deprecated one-string method target.
#[test]
fn execute_program_reflection_method_accepts_single_method_string() {
    let program = parse_fragment(
        br#"class EvalReflectCtorMethodTarget {
    public function MiXeDCase() { return "ok"; }
}
$ref = new ReflectionMethod(objectOrMethod: "EvalReflectCtorMethodTarget::mixedcase");
echo $ref->getDeclaringClass()->getName(); echo ":";
echo $ref->getName(); echo ":";
echo $ref->invoke(new EvalReflectCtorMethodTarget());
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "EvalReflectCtorMethodTarget:MiXeDCase:ok");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod construction throws catchable PHP reflection errors.
#[test]
fn execute_program_reflection_method_constructor_throws_reflection_exceptions() {
    let program = parse_fragment(
        br#"class EvalReflectMissingMethodTarget {}
try {
    new ReflectionMethod("EvalReflectMissingMethodTarget", "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    new ReflectionMethod("EvalReflectMissingMethodTarget::missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    ReflectionMethod::createFromMethodName("EvalReflectMissingMethodTarget::missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionMethod("EvalReflectMissingClass", "run");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    ReflectionMethod::createFromMethodName("not-a-method");
    echo "bad";
} catch (ReflectionException $e) {
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
        "ReflectionException:Method EvalReflectMissingMethodTarget::missing() does not exist|Method EvalReflectMissingMethodTarget::missing() does not exist|Method EvalReflectMissingMethodTarget::missing() does not exist|Class \"EvalReflectMissingClass\" does not exist|ReflectionMethod::createFromMethodName(): Argument #1 ($method) must be a valid method name"
    );
    assert_eq!(values.get(result.expect("execute eval ir")), FakeValue::Bool(true));
}

/// Verifies ReflectionProperty construction throws catchable PHP reflection errors.
#[test]
fn execute_program_reflection_property_constructor_throws_reflection_exceptions() {
    let program = parse_fragment(
        br#"class EvalReflectMissingPropertyTarget {}
$object = new EvalReflectMissingPropertyTarget();
$object->dynamic = 1;
try {
    new ReflectionProperty("EvalReflectMissingPropertyTarget", "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    new ReflectionProperty("EvalReflectMissingPropertyClass", "value");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionProperty($object, "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
$property = new ReflectionProperty($object, "dynamic");
echo $property->getName();
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
        "ReflectionException:Property EvalReflectMissingPropertyTarget::$missing does not exist|Class \"EvalReflectMissingPropertyClass\" does not exist|Property EvalReflectMissingPropertyTarget::$missing does not exist|dynamic"
    );
    assert_eq!(values.get(result.expect("execute eval ir")), FakeValue::Bool(true));
}

/// Verifies ReflectionClassConstant construction throws catchable PHP reflection errors.
#[test]
fn execute_program_reflection_class_constant_constructor_throws_reflection_exceptions() {
    let program = parse_fragment(
        br#"class EvalReflectMissingConstantTarget {
    public const OK = 1;
}
try {
    new ReflectionClassConstant("EvalReflectMissingConstantTarget", "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    new ReflectionClassConstant("EvalReflectMissingConstantClass", "VALUE");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
echo (new ReflectionClassConstant("EvalReflectMissingConstantTarget", "OK"))->getName();
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
        "ReflectionException:Constant EvalReflectMissingConstantTarget::missing does not exist|Class \"EvalReflectMissingConstantClass\" does not exist|OK"
    );
    assert_eq!(values.get(result.expect("execute eval ir")), FakeValue::Bool(true));
}

/// Verifies ReflectionEnumUnitCase/BackedCase construction throws PHP reflection errors.
#[test]
fn execute_program_reflection_enum_case_constructor_throws_reflection_exceptions() {
    let program = parse_fragment(
        br#"enum EvalReflectMissingCaseUnit {
    case Ready;
    public const TOKEN = 1;
}
enum EvalReflectMissingCaseBacked: string {
    case Ready = "ready";
    public const TOKEN = 1;
}
class EvalReflectMissingCaseClass {
    public const TOKEN = 1;
}
try {
    new ReflectionEnumUnitCase("EvalReflectMissingCaseUnit", "Missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnumUnitCase("EvalReflectMissingCaseClass", "TOKEN");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnumUnitCase("EvalReflectMissingCaseUnit", "TOKEN");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnumBackedCase("EvalReflectMissingCaseUnit", "Ready");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnumBackedCase("EvalReflectMissingCaseBacked", "Missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnumBackedCase("EvalReflectMissingCaseClass", "Missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
echo (new ReflectionEnumUnitCase("EvalReflectMissingCaseBacked", "Ready"))->getName(); echo ":";
echo (new ReflectionEnumBackedCase("EvalReflectMissingCaseBacked", "Ready"))->getBackingValue();
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
        "ReflectionException:Constant EvalReflectMissingCaseUnit::Missing does not exist|Constant EvalReflectMissingCaseClass::TOKEN is not a case|Constant EvalReflectMissingCaseUnit::TOKEN is not a case|Enum case EvalReflectMissingCaseUnit::Ready is not a backed case|Constant EvalReflectMissingCaseBacked::Missing does not exist|Constant EvalReflectMissingCaseClass::Missing does not exist|Ready:ready"
    );
    assert_eq!(values.get(result.expect("execute eval ir")), FakeValue::Bool(true));
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

/// Verifies ReflectionClass stringifies retained eval class metadata.
#[test]
fn execute_program_reflection_class_to_string() {
    let program = parse_fragment(
        br#"class EvalReflectClassStringTarget {
    public const ANSWER = 42;
    public int $id = 7;
    public function read(string $name = "Ada"): ?string { return $name; }
}
$ref = new ReflectionClass("EvalReflectClassStringTarget");
echo $ref;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Class [ <user> class EvalReflectClassStringTarget ] {\n  - Constants [1] {\n    Constant [ public int ANSWER ] { 42 }\n  }\n  - Properties [1] {\n    Property [ public int $id = 7 ]\n  }\n  - Methods [1] {\n    Method [ <user> public method read ]\n  }\n}\n"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
