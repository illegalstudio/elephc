//! Purpose:
//! Interpreter tests for eval class metadata and relation builtins.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - Eval class declarations expose parent/interface metadata while trait and
//!   attribute metadata remains empty.
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

/// Verifies class attribute helpers expose empty metadata arrays in eval.
#[test]
fn execute_program_dispatches_class_attribute_metadata_builtins() {
    let program = parse_fragment(
        br#"class EvalAttrMeta {}
$names = class_attribute_names("EvalAttrMeta");
echo is_array($names) && count($names) === 0 ? "names" : "bad"; echo ":";
$attrs = class_get_attributes("EvalAttrMeta");
echo is_array($attrs) && count($attrs) === 0 ? "attrs" : "bad"; echo ":";
$args = class_attribute_args("EvalAttrMeta", "DemoAttr");
echo is_array($args) && count($args) === 0 ? "args" : "bad"; echo ":";
$call_names = call_user_func("class_attribute_names", "EvalAttrMeta");
echo is_array($call_names) && count($call_names) === 0 ? "callnames" : "bad"; echo ":";
$call_args = call_user_func_array(
    "class_attribute_args",
    ["class_name" => "EvalAttrMeta", "attribute_name" => "DemoAttr"]
);
echo is_array($call_args) && count($call_args) === 0 ? "callargs" : "bad"; echo ":";
echo function_exists("class_attribute_names"); echo function_exists("class_get_attributes");
echo function_exists("class_attribute_args");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "names:attrs:args:callnames:callargs:111");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
