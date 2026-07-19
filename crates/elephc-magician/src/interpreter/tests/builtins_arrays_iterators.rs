//! Purpose:
//! Interpreter tests for iterator-style array builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases exercise callback iteration and object iterator dispatch.

use super::super::*;
use super::support::*;

/// Verifies eval iterator array helpers support direct and dynamic builtin calls.
#[test]
fn execute_program_dispatches_iterator_array_builtins() {
    let program = parse_fragment(
            br#"$items = ["x" => 1, "y" => 2];
$copy = iterator_to_array($items);
echo iterator_count($items) . ":" . $copy["x"] . $copy["y"] . ":";
$values = iterator_to_array($items, false);
echo (isset($values["x"]) ? "bad" : "reindexed") . ":" . $values[0] . $values[1] . ":";
echo call_user_func("iterator_count", $items) . ":";
$spread = call_user_func_array("iterator_to_array", ["iterator" => $items, "preserve_keys" => false]);
echo $spread[0] . $spread[1] . ":";
return function_exists("iterator_count") && function_exists("iterator_to_array");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:12:reindexed:12:2:12:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `iterator_apply()` drives Iterator objects and callback args.
#[test]
fn execute_program_dispatches_iterator_apply_object_builtin() {
    let program = parse_fragment(
        br#"function eval_apply($prefix) { echo $prefix; return true; }
echo iterator_apply($it, "eval_apply", ["prefix" => "x"]) . ":";
echo call_user_func("iterator_apply", $it, "eval_apply", ["y"]) . ":";
return function_exists("iterator_apply");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let iterator = values.alloc(FakeValue::Iterator {
        len: 3,
        position: 0,
    });
    scope.set("it", iterator, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "xxx3:yyy3:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `iterator_apply()` accepts object-method callable arrays.
#[test]
fn execute_program_iterator_apply_dispatches_object_method_array() {
    let program = parse_fragment(
        br#"$box = new Box(5);
echo iterator_apply($it, [$box, "add_x"], [1]) . ":";
return call_user_func("iterator_apply", $it, [$box, "add_x"], [1]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let iterator = values.alloc(FakeValue::Iterator {
        len: 3,
        position: 0,
    });
    scope.set("it", iterator, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:");
    assert_eq!(values.get(result), FakeValue::Int(3));
}
/// Verifies eval `iterator_apply()` counts the position where the callback stops.
#[test]
fn execute_program_iterator_apply_stops_on_falsey_callback() {
    let program = parse_fragment(
        br#"function eval_stop() { echo "s"; return false; }
return iterator_apply($it, "eval_stop");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let iterator = values.alloc(FakeValue::Iterator {
        len: 3,
        position: 0,
    });
    scope.set("it", iterator, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "s");
    assert_eq!(values.get(result), FakeValue::Int(1));
}
/// Verifies eval `array_filter()` removes falsey values while preserving original keys.
#[test]
fn execute_program_dispatches_array_filter_builtin() {
    let program = parse_fragment(
        br#"$filtered = array_filter([0, 1, 2, "", false, null, "0", "ok"]);
echo count($filtered) . ":" . $filtered[1] . ":" . $filtered[2] . ":" . $filtered[7] . ":";
$assoc = array_filter(["a" => 0, "b" => 2, "c" => ""]);
echo (array_key_exists("a", $assoc) ? "bad" : "drop") . ":" . $assoc["b"] . ":";
$null = array_filter([0, 3], null, 1);
echo count($null) . ":" . $null[1] . ":";
$call = call_user_func("array_filter", [0, 4]);
echo count($call) . ":" . $call[1] . ":";
$spread = call_user_func_array("array_filter", ["array" => [0, 5], "callback" => null]);
echo count($spread) . ":" . $spread[1] . ":";
function eval_keep_even($value) { return $value % 2 == 0; }
$evens = array_filter([1, 2, 3, 4], "eval_keep_even");
echo count($evens) . ":" . $evens[1] . ":" . $evens[3] . ":";
function eval_keep_key($key) { return $key === "b"; }
$keyed = array_filter(["a" => 10, "b" => 20], "eval_keep_key", ARRAY_FILTER_USE_KEY);
echo count($keyed) . ":" . $keyed["b"] . ":";
function eval_keep_both($value, $key) { return $key === "c" || $value === 1; }
$both = array_filter(["a" => 1, "b" => 2, "c" => 3], "eval_keep_both", ARRAY_FILTER_USE_BOTH);
echo count($both) . ":" . $both["a"] . ":" . $both["c"] . ":";
$ints = array_filter([1, "x", 2], "is_int");
echo count($ints) . ":" . $ints[0] . ":" . $ints[2] . ":";
return function_exists("array_filter");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "3:1:2:ok:drop:2:1:3:1:4:1:5:2:2:4:1:20:2:1:3:2:1:2:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
