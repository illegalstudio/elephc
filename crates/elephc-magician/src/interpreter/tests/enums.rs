//! Purpose:
//! Interpreter tests for eval-declared enum runtime behavior.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases verify enum singleton cases, class-like metadata, backed values,
//!   and method/interface dispatch through the eval object path.

use super::super::*;
use super::support::*;

/// Executes an eval enum fragment and asserts it fails during runtime validation.
fn assert_invalid_enum_fragment(source: &[u8], message: &str) {
    let program = parse_fragment(source).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values).expect_err(message);

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies pure eval enums expose singleton cases and class-like introspection.
#[test]
fn execute_program_declares_pure_eval_enum_cases() {
    let program = parse_fragment(
        br#"enum EvalSuit {
    case Hearts;
    case Clubs;
}
$cases = EvalSuit::cases();
echo enum_exists("evalsuit") ? "enum" : "bad"; echo ":";
echo class_exists("EvalSuit") ? "class" : "bad"; echo ":";
echo count($cases); echo ":";
echo $cases[0] === EvalSuit::Hearts ? "same" : "bad"; echo ":";
echo EvalSuit::Hearts->name; echo ":";
return get_class(EvalSuit::Clubs);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "enum:class:2:same:Hearts:");
    assert_eq!(
        values.get(result),
        FakeValue::String("EvalSuit".to_string())
    );
}

/// Verifies backed eval enums expose values and `from` / `tryFrom` lookups.
#[test]
fn execute_program_declares_backed_eval_enum_cases() {
    let program = parse_fragment(
        br#"enum EvalColor: int {
    case Red = 1;
    case Green = 2;
}
echo EvalColor::Green->value; echo ":";
echo EvalColor::from(value: 1) === EvalColor::Red ? "from" : "bad"; echo ":";
return EvalColor::tryFrom(99);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:from:");
    assert_eq!(values.get(result), FakeValue::Null);
}

/// Verifies eval enum `from()` misses throw catchable `ValueError` objects.
#[test]
fn execute_program_enum_from_miss_throws_value_error() {
    let program = parse_fragment(
        br#"enum EvalColor: int {
    case Red = 1;
}
try {
    EvalColor::from(99);
    echo "bad";
} catch (ValueError $e) {
    echo get_class($e) . ":" . $e->getMessage();
    return true;
}
return false;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "ValueError:99 is not a valid backing value for enum EvalColor"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval enum methods, constants, and interface implementation dispatch.
#[test]
fn execute_program_dispatches_eval_enum_methods_and_interfaces() {
    let program = parse_fragment(
        br#"interface EvalLabel {
    function label();
}
enum EvalSuit implements EvalLabel {
    case Hearts;
    public const PREFIX = "suit";
    public function label() { return self::PREFIX . ":" . $this->name; }
    public static function fallback() { return self::Hearts; }
}
echo is_a(EvalSuit::Hearts, "EvalLabel") ? "iface" : "bad"; echo ":";
echo EvalSuit::Hearts->label(); echo ":";
return EvalSuit::fallback() === EvalSuit::Hearts;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "iface:suit:Hearts:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval enums can import trait methods and expose direct trait metadata.
#[test]
fn execute_program_dispatches_eval_enum_trait_use() {
    let program = parse_fragment(
        br#"trait EvalEnumTrait {
    public function label() { return $this->name; }
    public static function suffix() { return "S"; }
}
enum EvalTraitEnum {
    use EvalEnumTrait {
        label as private hiddenLabel;
    }
    case Ready;
    public function read() { return $this->label() . ":" . $this->hiddenLabel(); }
}
echo EvalTraitEnum::Ready->read(); echo ":";
echo EvalTraitEnum::suffix(); echo ":";
$ref = new ReflectionClass("EvalTraitEnum");
$traits = $ref->getTraitNames();
echo count($traits); echo ":"; echo $traits[0]; echo ":";
$aliases = $ref->getTraitAliases();
echo $aliases["hiddenLabel"]; echo ":";
$uses = class_uses(EvalTraitEnum::Ready);
echo count($uses); echo ":"; echo $uses["EvalEnumTrait"]; echo ":";
return EvalTraitEnum::Ready->label();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Ready:Ready:S:1:EvalEnumTrait:EvalEnumTrait::label:1:EvalEnumTrait:"
    );
    assert_eq!(values.get(result), FakeValue::String("Ready".to_string()));
}

/// Verifies enum synthetic methods hide conflicting trait imports like PHP.
#[test]
fn execute_program_uses_enum_synthetic_methods_over_trait_imports() {
    let program = parse_fragment(
        br#"trait EvalEnumSyntheticTrait {
    public function cases() { return "trait-cases"; }
    public static function from($value) { return "trait-from"; }
    public static function tryFrom($value) { return "trait-try"; }
}
enum EvalPureSynthetic {
    use EvalEnumSyntheticTrait {
        cases as traitCases;
    }
    case Ready;
}
enum EvalBackedSynthetic: string {
    use EvalEnumSyntheticTrait {
        cases as traitCases;
        from as traitFrom;
    }
    case Ready = "ready";
}
echo is_array(EvalPureSynthetic::Ready->cases()) ? "cases" : "bad"; echo ":";
echo EvalPureSynthetic::Ready->traitCases(); echo ":";
echo EvalPureSynthetic::from("x"); echo ":";
echo EvalPureSynthetic::Ready->from("x"); echo ":";
echo EvalPureSynthetic::tryFrom("x"); echo ":";
echo EvalBackedSynthetic::from("ready")->value; echo ":";
echo EvalBackedSynthetic::Ready->from("ready")->value; echo ":";
echo EvalBackedSynthetic::tryFrom("missing") === null ? "null" : "bad"; echo ":";
echo EvalBackedSynthetic::traitFrom("x"); echo ":";
echo EvalBackedSynthetic::Ready->traitCases(); echo ":";
echo is_callable([EvalBackedSynthetic::Ready, "cases"]) ? "callable" : "bad";"#,
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
        "cases:trait-cases:trait-from:trait-from:trait-try:ready:ready:null:trait-from:trait-cases:callable"
    );
}

/// Verifies pure eval enums may declare `from` and `tryFrom` methods directly.
#[test]
fn execute_program_allows_pure_eval_enum_direct_from_methods() {
    let program = parse_fragment(
        br#"enum EvalPureDirectFrom {
    case Ready;
    public static function from($value) { return "from:" . $value; }
    public static function tryFrom($value) { return "try:" . $value; }
}
echo EvalPureDirectFrom::from("x"); echo ":";
echo EvalPureDirectFrom::Ready->tryFrom("y"); echo ":";
return is_callable([EvalPureDirectFrom::Ready, "from"]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "from:x:try:y:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionMethod metadata and invocation for enum synthetic methods.
#[test]
fn execute_program_reflects_eval_enum_synthetic_methods() {
    let program = parse_fragment(
        br#"enum EvalReflectSyntheticEnum: string {
    case Ready = "ready";
}
enum EvalReflectPureSyntheticEnum {
    case Ready;
}
$ref = new ReflectionClass("EvalReflectSyntheticEnum");
$methods = $ref->getMethods(ReflectionMethod::IS_STATIC);
echo count($methods); echo ":";
echo $methods[0]->getName(); echo "/";
echo $methods[1]->getName(); echo "/";
echo $methods[2]->getName(); echo ":";
$cases = $ref->getMethod("cases");
echo $cases->getReturnType(); echo ":";
echo count($cases->invoke(null)); echo ":";
$from = new ReflectionMethod("EvalReflectSyntheticEnum", "from");
$params = $from->getParameters();
echo $from->getDeclaringClass()->getName(); echo ":";
echo $from->getNumberOfParameters(); echo "/";
echo $from->getNumberOfRequiredParameters(); echo ":";
echo $params[0]->getName(); echo "/";
echo $params[0]->getType(); echo ":";
echo $from->getReturnType(); echo ":";
echo $from->invoke(null, "ready")->name; echo ":";
$try = ReflectionMethod::createFromMethodName("EvalReflectSyntheticEnum::tryFrom");
echo $try->getReturnType(); echo ":";
echo $try->invokeArgs(null, ["missing"]) === null ? "null" : "bad"; echo ":";
$pure = new ReflectionClass("EvalReflectPureSyntheticEnum");
echo count($pure->getMethods()); echo ":";
echo $pure->hasMethod("from") ? "bad" : "nofrom";"#,
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
        "3:cases/from/tryFrom:array:1:EvalReflectSyntheticEnum:1/1:value/string|int:static:Ready:?static:null:1:nofrom"
    );
}

/// Verifies eval enum interfaces can inherit PHP's native enum marker interfaces.
#[test]
fn execute_program_allows_eval_enum_marker_interface_inheritance() {
    let program = parse_fragment(
        br#"interface EvalUnitMarker extends UnitEnum {}
interface EvalBackedMarker extends BackedEnum {}
enum EvalMarkedUnit implements EvalUnitMarker {
    case Ready;
}
enum EvalMarkedBacked: string implements EvalBackedMarker {
    case Ready = "ready";
}
echo interface_exists("UnitEnum") ? "U" : "u"; echo ":";
echo interface_exists("BackedEnum") ? "B" : "b"; echo ":";
echo is_a(EvalMarkedUnit::Ready, "EvalUnitMarker") ? "unit" : "bad"; echo ":";
echo is_a(EvalMarkedBacked::Ready, "EvalBackedMarker") ? "backed" : "bad"; echo ":";
$unitInterfaces = class_implements("EvalMarkedUnit");
echo count($unitInterfaces); echo ":"; echo $unitInterfaces["EvalUnitMarker"]; echo ":";
echo $unitInterfaces["UnitEnum"]; echo ":";
$backedInterfaces = (new ReflectionClass("EvalMarkedBacked"))->getInterfaceNames();
echo count($backedInterfaces); echo ":"; echo $backedInterfaces[0]; echo ":";
echo $backedInterfaces[1]; echo ":"; echo $backedInterfaces[2]; echo ":";
return EvalMarkedBacked::Ready->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "U:B:unit:backed:2:EvalUnitMarker:UnitEnum:3:EvalBackedMarker:UnitEnum:BackedEnum:"
    );
    assert_eq!(
        values.get(result),
        FakeValue::String("ready".to_string())
    );
}

/// Verifies eval rejects enum members that conflict with PHP enum rules.
#[test]
fn execute_program_rejects_invalid_eval_enum_members() {
    assert_invalid_enum_fragment(
        br#"enum EvalInvalidEnum {
    case Ready;
    public const Ready = 1;
}"#,
        "case and constant name collision should fail",
    );

    assert_invalid_enum_fragment(
        br#"enum EvalInvalidEnumMethod {
    case Ready;
    public static function cases() { return []; }
}"#,
        "reserved enum method should fail",
    );

    assert_invalid_enum_fragment(
        br#"enum EvalInvalidBackedEnumMethod: string {
    case Ready = "ready";
    public static function from($value) { return self::Ready; }
}"#,
        "backed enum from method should fail",
    );

    assert_invalid_enum_fragment(
        br#"enum EvalInvalidEnumMagicMethod {
    case Ready;
    public function __destruct() {}
}"#,
        "forbidden enum magic method should fail",
    );

    assert_invalid_enum_fragment(
        br#"enum EvalInvalidEnumMagicMethodCase {
    case Ready;
    public function __GET($name) {}
}"#,
        "forbidden enum magic method lookup should be case-insensitive",
    );

    assert_invalid_enum_fragment(
        br#"trait EvalInvalidEnumPropertyTrait {
    public int $x = 1;
}
enum EvalInvalidEnumTraitProperty {
    use EvalInvalidEnumPropertyTrait;
    case Ready;
}"#,
        "enum cannot import trait properties",
    );

    assert_invalid_enum_fragment(
        br#"enum EvalInvalidExplicitUnitEnum implements UnitEnum {
    case Ready;
}"#,
        "enum cannot explicitly implement UnitEnum",
    );

    assert_invalid_enum_fragment(
        br#"enum EvalInvalidExplicitBackedEnum: string implements BackedEnum {
    case Ready = "ready";
}"#,
        "enum cannot explicitly implement BackedEnum",
    );

    assert_invalid_enum_fragment(
        br#"interface EvalBackedMarker extends BackedEnum {}
enum EvalInvalidPureBackedMarker implements EvalBackedMarker {
    case Ready;
}"#,
        "pure enum cannot implement BackedEnum through a marker",
    );

    assert_invalid_enum_fragment(
        br#"interface EvalUnitMarker extends UnitEnum {}
class EvalInvalidUnitEnumClass implements EvalUnitMarker {}"#,
        "non-enum class cannot implement UnitEnum through a marker",
    );
}

/// Verifies eval allows the enum magic methods PHP permits.
#[test]
fn execute_program_allows_supported_eval_enum_magic_methods() {
    let program = parse_fragment(
        br#"enum EvalAllowedEnumMagic {
    case Ready;
    public function __call($name, $arguments) { return $name; }
    public static function __callStatic($name, $arguments) { return $name; }
    public function __invoke() { return "invoke"; }
}
return enum_exists("EvalAllowedEnumMagic");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Bool(true));
}
