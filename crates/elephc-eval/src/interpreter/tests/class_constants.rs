//! Purpose:
//! Interpreter tests for eval-declared class constants.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases cover inherited lookup, scoped receivers, visibility, and dynamic storage.

use super::super::*;
use super::support::*;

/// Verifies class constants can be fetched directly and through scoped receivers.
#[test]
fn execute_program_reads_eval_class_constants() {
    let program = parse_fragment(
        br#"class EvalConstBase {
    public const SEED = 2;
    protected const HIDDEN = 5;
    public static function read() {
        return self::SEED + static::SEED;
    }
    public static function hidden() {
        return self::HIDDEN;
    }
}
class EvalConstChild extends EvalConstBase {
    public const SEED = 7;
    public static function readParent() {
        return parent::SEED;
    }
}
echo EvalConstBase::SEED; echo ":";
echo EvalConstChild::SEED; echo ":";
echo EvalConstChild::read(); echo ":";
echo EvalConstChild::readParent(); echo ":";
return EvalConstChild::hidden();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:7:9:2:");
    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies protected class constants are not readable from global eval scope.
#[test]
fn execute_program_rejects_protected_eval_class_constant_from_global_scope() {
    let program = parse_fragment(
        br#"class EvalConstProtected {
    protected const SECRET = 4;
}
return EvalConstProtected::SECRET;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values)
        .expect_err("global protected class constant access should fail");
}

/// Verifies duplicate class constants in one eval class are rejected.
#[test]
fn execute_program_rejects_duplicate_eval_class_constant() {
    let program = parse_fragment(
        br#"class EvalConstDuplicate {
    public const SEED = 1;
    public const SEED = 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values)
        .expect_err("duplicate class constant should fail");
}
