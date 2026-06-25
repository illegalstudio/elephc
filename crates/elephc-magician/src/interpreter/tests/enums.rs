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
