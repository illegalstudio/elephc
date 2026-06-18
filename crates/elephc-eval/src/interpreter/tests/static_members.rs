//! Purpose:
//! Interpreter tests for eval-declared static properties and static methods.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases cover storage persistence, visibility checks, and late static binding.

use super::super::*;
use super::support::*;

/// Verifies static properties persist and can be read and written through static methods.
#[test]
fn execute_program_reads_writes_eval_static_members() {
    let program = parse_fragment(
        br#"class EvalStaticCounter {
    public static int $count = 1;
    public static function bump($step) {
        self::$count += $step;
        return self::$count;
    }
}
echo EvalStaticCounter::$count; echo ":";
echo EvalStaticCounter::bump(2); echo ":";
return EvalStaticCounter::$count;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:3:");
    assert_eq!(values.get(result), FakeValue::Int(3));
}

/// Verifies `static::` uses the called class while `self::` keeps the declaring class.
#[test]
fn execute_program_late_binds_eval_static_property_access() {
    let program = parse_fragment(
        br#"class EvalStaticBase {
    protected static int $n = 2;
    public static function add($x) {
        static::$n += $x;
        return static::$n;
    }
    public static function baseRead() {
        return self::$n;
    }
}
class EvalStaticChild extends EvalStaticBase {
    protected static int $n = 10;
}
echo EvalStaticChild::add(4); echo ":";
echo EvalStaticBase::add(3); echo ":";
return EvalStaticBase::baseRead();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "14:5:");
    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies private static properties are not readable from global eval scope.
#[test]
fn execute_program_rejects_private_eval_static_property_from_global_scope() {
    let program = parse_fragment(
        br#"class EvalStaticPrivate {
    private static int $secret = 4;
}
return EvalStaticPrivate::$secret;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values)
        .expect_err("global private static property access should fail");
}

/// Verifies eval rejects static-style calls to non-static methods.
#[test]
fn execute_program_rejects_static_call_to_eval_instance_method() {
    let program = parse_fragment(
        br#"class EvalStaticCallRules {
    public function read() { return 1; }
}
return EvalStaticCallRules::read();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values)
        .expect_err("static call to instance method should fail");
}

/// Verifies eval rejects object-style calls to static methods.
#[test]
fn execute_program_rejects_instance_call_to_eval_static_method() {
    let program = parse_fragment(
        br#"class EvalStaticInstanceRules {
    public static function read() { return 1; }
}
$box = new EvalStaticInstanceRules();
return $box->read();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values)
        .expect_err("instance call to static method should fail");
}
