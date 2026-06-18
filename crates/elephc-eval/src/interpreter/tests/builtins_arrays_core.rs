//! Purpose:
//! Interpreter tests for array aggregation, mapping, mutation, and sorting builtins.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases cover array builtins that transform or reorder array values.

use super::super::*;
use super::support::*;

/// Verifies eval `ord()` returns the first byte and supports callable dispatch.
#[test]
fn execute_program_dispatches_ord_builtin() {
    let program = parse_fragment(
        br#"echo ord("A");
echo ":" . ord("");
echo ":" . call_user_func("ord", "B");
echo ":" . call_user_func_array("ord", ["C"]);
echo ":"; echo function_exists("ord");
return ord("Z");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "65:0:66:67:1");
    assert_eq!(values.get(result), FakeValue::Int(90));
}
/// Verifies eval array aggregate builtins iterate array values and support callable dispatch.
#[test]
fn execute_program_dispatches_array_aggregate_builtins() {
    let program = parse_fragment(
        br#"echo array_sum([1, 2, 3]);
echo ":" . array_product([2, 3, 4]);
echo ":" . array_sum([]);
echo ":" . array_product([]);
echo ":" . array_sum(["a" => 2, "b" => 5]);
echo ":" . call_user_func("array_sum", [3, 4]);
echo ":" . call_user_func_array("array_product", [[2, 5]]);
echo ":"; echo function_exists("array_sum");
return function_exists("array_product");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "6:24:0:1:7:7:10:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `array_map()` applies callbacks and preserves source keys.
#[test]
fn execute_program_dispatches_array_map_builtin() {
    let program = parse_fragment(
        br#"function eval_map_double($value) { return $value * 2; }
$mapped = array_map("eval_map_double", [1, 2, 3]);
echo $mapped[0] . ":" . $mapped[2] . ":";
$assoc = array_map("strtoupper", ["a" => "x", "b" => "y"]);
echo $assoc["a"] . ":" . $assoc["b"] . ":";
$identity = array_map(null, ["k" => "v"]);
echo $identity["k"] . ":";
function eval_map_pair($left, $right) { return $left . "-" . ($right ?? "N"); }
$pairs = array_map("eval_map_pair", ["a" => "L", "b" => "R"], ["x" => "1"]);
echo $pairs[0] . ":" . $pairs[1] . ":";
$zipped = array_map(null, [1, 2], [3, 4]);
echo $zipped[0][0] . $zipped[0][1] . ":" . $zipped[1][0] . $zipped[1][1] . ":";
$call = call_user_func("array_map", "intval", ["7"]);
echo $call[0] . ":";
$multi_call = call_user_func("array_map", "eval_map_pair", ["Q"], ["9"]);
echo $multi_call[0] . ":";
$spread = call_user_func_array("array_map", ["callback" => "strval", "array" => [8]]);
echo $spread[0] . ":";
return function_exists("array_map");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:6:X:Y:v:L-1:R-N:13:24:7:Q-9:8:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `array_reduce()` folds values through a string callback.
#[test]
fn execute_program_dispatches_array_reduce_builtin() {
    let program = parse_fragment(
            br#"function eval_reduce_sum($carry, $item) { return $carry + $item; }
echo array_reduce([1, 2, 3], "eval_reduce_sum", 10) . ":";
function eval_reduce_join($carry, $item) { return $carry . $item; }
echo array_reduce([4, 5], "eval_reduce_sum") . ":";
echo array_reduce(["a", "b"], "eval_reduce_join", "") . ":";
$named = array_reduce(array: [6, 7], callback: "eval_reduce_sum");
echo $named . ":";
$call = call_user_func("array_reduce", [4, 5], "eval_reduce_sum");
echo $call . ":";
$spread = call_user_func_array("array_reduce", ["array" => [2, 3], "callback" => "eval_reduce_sum", "initial" => 4]);
echo $spread . ":";
return function_exists("array_reduce");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "16:9:ab:13:9:9:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `array_walk()` invokes string callbacks with value and key cells.
#[test]
fn execute_program_dispatches_array_walk_builtin() {
    let program = parse_fragment(
            br#"function eval_walk_show($value, $key) { echo $key . "=" . $value . ";"; }
echo array_walk(["a" => 2, "b" => 3], "eval_walk_show") ? "T:" : "F:";
$call = call_user_func("array_walk", [4, 5], "eval_walk_show");
$spread = call_user_func_array("array_walk", ["array" => ["z" => 6], "callback" => "eval_walk_show"]);
return function_exists("array_walk");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "a=2;b=3;T:0=4;1=5;z=6;");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `array_pop()` and `array_shift()` write back only for direct variable calls.
#[test]
fn execute_program_dispatches_array_pop_shift_builtins() {
    let program = parse_fragment(
        br#"$a = [1, 2, 3];
echo array_pop($a) . ":" . count($a) . ":" . $a[1] . ":";
$b = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
echo array_shift(array: $b) . ":" . $b[0] . ":" . $b["y"] . ":" . $b[1] . ":";
$c = [4, 5];
echo call_user_func("array_pop", $c) . ":" . count($c) . ":" . $c[1] . ":";
$d = [6, 7];
echo call_user_func_array("array_shift", ["array" => $d]) . ":" . count($d) . ":" . $d[0] . ":";
return function_exists("array_pop") && function_exists("array_shift");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:2:2:1:2:3:4:5:2:5:6:2:6:");
    assert_eq!(
        values.warnings,
        vec![
            "array_pop(): Argument #1 ($array) must be passed by reference, value given",
            "array_shift(): Argument #1 ($array) must be passed by reference, value given",
        ]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `array_push()` and `array_unshift()` write back direct variable calls.
#[test]
fn execute_program_dispatches_array_push_unshift_builtins() {
    let program = parse_fragment(
        br#"$a = [1];
echo array_push($a, 2, 3) . ":" . $a[2] . ":";
$b = ["x" => 1, 10 => 2];
echo array_push($b, "A") . ":" . $b["x"] . ":" . $b[11] . ":";
$c = [2, 3];
echo array_unshift($c, 0, 1) . ":" . $c[0] . ":" . $c[3] . ":";
$d = ["x" => 1, 10 => 2, "y" => 3];
echo array_unshift($d, "A") . ":" . $d[0] . ":" . $d["x"] . ":" . $d[1] . ":" . $d["y"] . ":";
$e = [5];
echo call_user_func("array_push", $e, 6) . ":" . count($e) . ":" . $e[0] . ":";
$f = [7];
echo call_user_func_array("array_unshift", [$f, 6]) . ":" . count($f) . ":" . $f[0] . ":";
return function_exists("array_push") && function_exists("array_unshift");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:3:3:1:A:4:0:3:4:A:1:2:3:2:1:5:2:1:7:");
    assert_eq!(
        values.warnings,
        vec![
            "array_push(): Argument #1 ($array) must be passed by reference, value given",
            "array_unshift(): Argument #1 ($array) must be passed by reference, value given",
        ]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `array_splice()` returns removed values and writes back direct variable calls.
#[test]
fn execute_program_dispatches_array_splice_builtin() {
    let program = parse_fragment(
            br#"$a = [10, 20, 30, 40];
$removed = array_splice($a, 1, 2);
echo count($removed) . ":" . $removed[0] . ":" . $removed[1] . ":" . count($a) . ":" . $a[1] . ":";
$b = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
$cut = array_splice(array: $b, offset: 1, length: 2);
echo $cut[0] . ":" . $cut["y"] . ":" . $b["x"] . ":" . $b[0] . ":";
$c = [1, 2, 3, 4];
$tail = call_user_func("array_splice", $c, -2, 1);
echo $tail[0] . ":" . count($c) . ":" . $c[2] . ":";
$d = [5, 6, 7];
$all = call_user_func_array("array_splice", ["array" => $d, "offset" => 1]);
echo count($all) . ":" . $all[0] . ":" . $all[1] . ":" . count($d) . ":";
$e = [1, 2, 3, 4];
$rep = array_splice($e, 1, 2, ["A", "B"]);
echo count($rep) . ":" . $rep[0] . ":" . $rep[1] . ":" . $e[0] . ":" . $e[1] . ":" . $e[2] . ":" . $e[3] . ":";
$f = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
$rep2 = array_splice(array: $f, offset: 1, length: 2, replacement: ["s" => "S", 9 => "N"]);
echo $rep2[0] . ":" . $rep2["y"] . ":" . $f["x"] . ":" . $f[0] . ":" . $f[1] . ":" . $f[2] . ":";
$g = [1, 2, 3];
$rep3 = array_splice($g, offset: 1, replacement: [9]);
echo count($rep3) . ":" . $rep3[0] . ":" . $rep3[1] . ":" . count($g) . ":" . $g[1] . ":";
$h = [1, 2, 3];
$removed2 = call_user_func_array("array_splice", ["array" => $h, "offset" => 1, "replacement" => [9]]);
echo count($removed2) . ":" . $removed2[0] . ":" . $removed2[1] . ":" . count($h) . ":" . $h[1] . ":";
return function_exists("array_splice");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "2:20:30:2:40:2:3:1:4:3:4:3:2:6:7:3:2:2:3:1:A:B:4:2:3:1:S:N:4:2:2:3:2:9:2:2:3:3:2:"
    );
    assert_eq!(
        values.warnings,
        vec![
            "array_splice(): Argument #1 ($array) must be passed by reference, value given",
            "array_splice(): Argument #1 ($array) must be passed by reference, value given",
            "array_splice(): Argument #1 ($array) must be passed by reference, value given",
        ]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `sort()` and `rsort()` reindex direct variable arrays only.
#[test]
fn execute_program_dispatches_sort_builtins() {
    let program = parse_fragment(
        br#"$a = [3, 1, 2];
echo sort($a) . ":" . $a[0] . $a[1] . $a[2] . ":";
$b = ["banana", "apple", "cherry"];
echo rsort(array: $b) . ":" . $b[0] . ":" . $b[2] . ":";
$c = ["x" => 3, "y" => 1, "z" => 2];
sort($c);
echo $c[0] . $c[1] . $c[2] . ":";
$d = [3, 1, 2];
echo call_user_func("sort", $d) . ":" . $d[0] . $d[1] . $d[2] . ":";
$e = [1, 2, 3];
echo call_user_func_array("rsort", ["array" => $e]) . ":" . $e[0] . ":" . $e[2] . ":";
return function_exists("sort") && function_exists("rsort");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:123:1:cherry:apple:123:1:312:1:1:3:");
    assert_eq!(
        values.warnings,
        vec![
            "sort(): Argument #1 ($array) must be passed by reference, value given",
            "rsort(): Argument #1 ($array) must be passed by reference, value given",
        ]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval key-preserving array ordering builtins write back direct variable calls.
#[test]
fn execute_program_dispatches_key_preserving_sort_builtins() {
    let program = parse_fragment(
            br#"$a = ["x" => 3, "y" => 1, "z" => 2];
echo asort($a) . ":";
foreach ($a as $key => $value) { echo $key . $value; }
echo ":";
$b = ["x" => 1, "y" => 3, "z" => 2];
echo arsort(array: $b) . ":";
foreach ($b as $key => $value) { echo $key . $value; }
echo ":";
$c = ["b" => 1, "a" => 2, 3 => 4];
echo ksort($c) . ":";
foreach ($c as $key => $value) { echo $key . $value; }
echo ":";
$d = ["b" => 1, "a" => 2, 3 => 4];
echo krsort($d) . ":";
foreach ($d as $key => $value) { echo $key . $value; }
echo ":";
$e = ["x" => 2, "y" => 1];
echo call_user_func("asort", $e) . ":" . $e["x"] . $e["y"] . ":";
$f = ["b" => 1, "a" => 2];
echo call_user_func_array("krsort", ["array" => $f]) . ":" . $f["b"] . $f["a"] . ":";
return function_exists("asort") && function_exists("arsort") && function_exists("ksort") && function_exists("krsort");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:y1z2x3:1:y3z2x1:1:34a2b1:1:b1a234:1:21:1:12:"
    );
    assert_eq!(
        values.warnings,
        vec![
            "asort(): Argument #1 ($array) must be passed by reference, value given",
            "krsort(): Argument #1 ($array) must be passed by reference, value given",
        ]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval natural sort builtins preserve keys and use natural string order.
#[test]
fn execute_program_dispatches_natural_sort_builtins() {
    let program = parse_fragment(
        br#"$a = ["img10", "img2", "img1"];
echo natsort($a) . ":";
foreach ($a as $key => $value) { echo $key . $value . ";"; }
echo ":";
$b = ["b" => "Img10", "a" => "img2", "c" => "IMG1"];
echo natcasesort(array: $b) . ":";
foreach ($b as $key => $value) { echo $key . $value . ";"; }
echo ":";
$c = ["x" => "b", "y" => "a"];
echo call_user_func("natsort", $c) . ":" . $c["x"] . $c["y"] . ":";
return function_exists("natsort") && function_exists("natcasesort");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:2img1;1img2;0img10;:1:cIMG1;aimg2;bImg10;:1:ba:"
    );
    assert_eq!(
        values.warnings,
        vec!["natsort(): Argument #1 ($array) must be passed by reference, value given"]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `shuffle()` reindexes direct variable arrays only.
#[test]
fn execute_program_dispatches_shuffle_builtin() {
    let program = parse_fragment(
            br#"$a = ["x" => 1, "y" => 2];
echo shuffle($a) . ":" . (isset($a["x"]) ? "bad" : "reindexed") . ":" . count($a) . ":" . array_sum($a) . ":";
$b = ["x" => 1, "y" => 2];
echo call_user_func("shuffle", $b) . ":" . $b["x"] . $b["y"] . ":";
return function_exists("shuffle");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:reindexed:2:3:1:12:");
    assert_eq!(
        values.warnings,
        vec!["shuffle(): Argument #1 ($array) must be passed by reference, value given"]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval user-comparator sort builtins call callbacks and write back direct arrays.
#[test]
fn execute_program_dispatches_user_sort_builtins() {
    let program = parse_fragment(
        br#"function eval_sort_cmp($left, $right) { echo "c"; return $left <=> $right; }
function eval_key_cmp($left, $right) { return strcmp($left, $right); }
$a = [3, 1, 2];
echo usort($a, "eval_sort_cmp") . ":";
foreach ($a as $value) { echo $value; }
echo ":";
$b = ["b" => 1, "a" => 3, "c" => 2];
echo uasort(array: $b, callback: "eval_sort_cmp") . ":";
foreach ($b as $key => $value) { echo $key . $value; }
echo ":";
$c = ["b" => 1, "a" => 2];
echo uksort($c, "eval_key_cmp") . ":";
foreach ($c as $key => $value) { echo $key . $value; }
echo ":";
$d = [2, 1];
echo call_user_func("usort", $d, "eval_sort_cmp") . ":" . $d[0] . $d[1] . ":";
return function_exists("usort") && function_exists("uasort") && function_exists("uksort");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "ccc1:123:ccc1:b1c2a3:1:a2b1:c1:21:");
    assert_eq!(
        values.warnings,
        vec!["usort(): Argument #1 ($array) must be passed by reference, value given"]
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
