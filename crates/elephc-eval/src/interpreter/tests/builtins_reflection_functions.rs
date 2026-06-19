//! Purpose:
//! Interpreter tests for eval-backed ReflectionFunction objects.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - Free eval functions retain only parameter names today, so required counts
//!   match the visible parameter list until richer function metadata exists.

use super::super::*;
use super::support::*;

/// Verifies eval-declared functions materialize ReflectionFunction parameter metadata.
#[test]
fn execute_program_reflects_eval_declared_function_parameters() {
    let program = parse_fragment(
        br#"function eval_reflect_free($left, $right) { return $left; }
$ref = new ReflectionFunction("eval_reflect_free");
$params = $ref->getParameters();
echo $ref->getName(); echo ":";
echo $ref->getNumberOfParameters(); echo ":";
echo $ref->getNumberOfRequiredParameters(); echo ":";
echo count($params); echo ":";
echo $params[0]->getName(); echo ":";
echo $params[1]->getPosition(); echo ":";
$declaring = $params[0]->getDeclaringFunction();
echo get_class($declaring); echo ":";
echo $declaring->getName();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "eval_reflect_free:2:2:2:left:1:ReflectionFunction:eval_reflect_free"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
