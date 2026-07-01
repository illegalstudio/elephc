//! Purpose:
//! End-to-end regressions for builtin lookup parity across AOT code and eval.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_builtin_parity` through Rust's test harness.
//!
//! Key details:
//! - Fixtures verify `function_exists()` and namespaced builtin fallback before
//!   and after eval has introduced dynamic symbols.

use std::fmt::Write;

use crate::support::compile_and_run;

/// Verifies AOT builtin lookup stays case-insensitive without eval being present.
#[test]
fn test_aot_function_exists_builtin_case_insensitive_without_eval() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("strlen") ? "S" : "s";
echo function_exists("STRLEN") ? "C" : "c";
echo function_exists("StRlEn") ? "M" : "m";
"#,
    );

    assert_eq!(out, "SCM");
}

/// Verifies eval declarations extend function lookup without hiding existing AOT builtins.
#[test]
fn test_function_exists_sees_builtins_and_eval_declared_functions_after_eval() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("eval_declared_lookup") ? "b" : "B";
eval('function eval_declared_lookup() { return "D"; }');
echo function_exists("strlen") ? "S" : "s";
echo function_exists("STRLEN") ? "C" : "c";
echo function_exists("eval_declared_lookup") ? eval_declared_lookup() : "d";
"#,
    );

    assert_eq!(out, "BSCD");
}

/// Verifies compiler-internal raw time helpers stay hidden from PHP function lookup.
#[test]
fn test_internal_raw_time_helpers_are_not_php_visible_before_or_after_eval() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("__elephc_mktime_raw") ? "M" : "m";
echo function_exists("__elephc_gmmktime_raw") ? "G" : "g";
echo function_exists("__elephc_strtotime_raw") ? "S" : "s";
eval('echo function_exists("__elephc_mktime_raw") ? "M" : "m";
echo function_exists("__elephc_gmmktime_raw") ? "G" : "g";
echo function_exists("__elephc_strtotime_raw") ? "S" : "s";');
"#,
    );

    assert_eq!(out, "mgsmgs");
}

/// Verifies eval builtin lookup remains case-insensitive after eval is active.
#[test]
fn test_eval_function_exists_builtin_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
eval('echo function_exists("strlen") ? "S" : "s";
echo function_exists("STRLEN") ? "C" : "c";
echo function_exists("StRlEn") ? "M" : "m";');
"#,
    );

    assert_eq!(out, "SCM");
}

/// Verifies eval `function_exists()` sees every compiler-catalog builtin name.
#[test]
fn test_eval_function_exists_covers_static_builtin_catalog() {
    let mut fragment = String::new();
    for name in elephc::builtin_metadata::php_visible_builtin_names() {
        writeln!(
            &mut fragment,
            "if (!function_exists(\"{name}\")) {{ echo \"{name},\"; }}"
        )
        .expect("write eval builtin probe");
    }
    fragment.push_str("return \"ok\";");

    let source = format!("<?php\necho eval({});\n", php_single_quoted_literal(&fragment));
    let out = compile_and_run(&source);

    assert_eq!(out, "ok");
}

/// Escapes a Rust string as a PHP single-quoted string literal.
fn php_single_quoted_literal(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('\'');
    for ch in value.chars() {
        match ch {
            '\\' => literal.push_str("\\\\"),
            '\'' => literal.push_str("\\'"),
            _ => literal.push(ch),
        }
    }
    literal.push('\'');
    literal
}

/// Verifies namespaced function calls fall back to builtins in AOT and eval code.
#[test]
fn test_namespaced_calls_fall_back_to_builtin_before_and_after_eval() {
    let out = compile_and_run(
        r#"<?php
namespace EvalBuiltinParity;
echo strlen("abc");
eval('namespace EvalBuiltinParity;
echo strlen("de");
echo STRLEN("fghi");');
"#,
    );

    assert_eq!(out, "324");
}

/// Verifies eval preg builtins use PCRE2 features that Rust regex did not support.
#[test]
fn test_eval_preg_uses_pcre2_lookaround_semantics() {
    let out = compile_and_run(
        r#"<?php
eval('echo preg_match("/foo(?=bar)/", "foobar");
echo ":";
echo preg_match("/(?<=foo)bar/", "foobar");');
"#,
    );

    assert_eq!(out, "1:1");
}

/// Verifies eval named builtin calls can skip optional parameters with defaults.
#[test]
fn test_eval_named_builtin_arguments_fill_default_gaps() {
    let out = compile_and_run(
        r#"<?php
eval('echo str_pad(string: "x", length: 3, pad_type: 0);
echo ":";
echo json_encode(value: ["a" => 1], depth: 512);');
"#,
    );

    assert_eq!(out, "  x:{\"a\":1}");
}

/// Verifies eval named builtin calls preserve variadic and by-reference behavior.
#[test]
fn test_eval_named_builtin_arguments_support_variadic_and_by_ref() {
    let out = compile_and_run(
        r#"<?php
eval('$items = [3, 1, 2];
sort(array: $items);
echo implode(",", $items);
echo ":";
echo max(value: 3, values: 8);');
"#,
    );

    assert_eq!(out, "1,2,3:8");
}

/// Verifies eval `call_user_func_array()` preserves positional ref-like builtin targets.
#[test]
fn test_eval_call_user_func_array_ref_like_builtins_write_back_positional_aliases() {
    let out = compile_and_run(
        r#"<?php
class EvalBuiltinRefBridgeBox {
    public array $items = [3, 1, 2];
    public static mixed $typed = "123";
}

eval('$items = [3, 1, 2];
echo call_user_func_array("sort", [&$items]) ? "S" : "s";
echo implode(",", $items) . "|";

$value = "42";
echo call_user_func_array("settype", [&$value, "integer"]) ? "T" : "t";
echo gettype($value) . ":" . $value . "|";

$box = new EvalBuiltinRefBridgeBox();
echo call_user_func_array("array_pop", [&$box->items]) . ":";
echo implode(",", $box->items) . "|";

echo call_user_func_array("settype", [&EvalBuiltinRefBridgeBox::$typed, "integer"]) ? "P" : "p";
echo gettype(EvalBuiltinRefBridgeBox::$typed) . ":" . EvalBuiltinRefBridgeBox::$typed;');
"#,
    );

    assert_eq!(out, "S1,2,3|Tinteger:42|2:3,1|Pinteger:123");
}

/// Verifies eval string-callable ref-like builtins write back through lvalue targets.
#[test]
fn test_eval_string_callable_ref_like_builtins_write_back_aliases() {
    let out = compile_and_run(
        r#"<?php
class EvalStringBuiltinRefBridgeBox {
    public array $items = [3, 1, 2];
    public static mixed $typed = "77";
}

eval('$sort = "sort";
$items = [3, 1, 2];
echo $sort($items) ? "S" : "s";
echo implode(",", $items) . "|";

$settype = "settype";
$value = "42";
echo $settype($value, "integer") ? "T" : "t";
echo gettype($value) . ":" . $value . "|";

$box = new EvalStringBuiltinRefBridgeBox();
$pop = "array_pop";
echo $pop($box->items) . ":" . implode(",", $box->items) . "|";

$setter = "settype";
echo $setter(EvalStringBuiltinRefBridgeBox::$typed, "integer") ? "P" : "p";
echo gettype(EvalStringBuiltinRefBridgeBox::$typed) . ":" . EvalStringBuiltinRefBridgeBox::$typed;');
"#,
    );

    assert_eq!(out, "S1,2,3|Tinteger:42|2:3,1|Pinteger:77");
}

/// Verifies eval `call_user_func_array()` preserves named ref-like builtin targets.
#[test]
fn test_eval_call_user_func_array_ref_like_builtins_write_back_named_aliases() {
    let out = compile_and_run(
        r#"<?php
eval('$matches = [];
echo call_user_func_array(
    "preg_match",
    ["pattern" => "/(a)(b)/", "subject" => "ab", "matches" => &$matches]
);
echo ":" . $matches[0] . ":" . $matches[1] . ":" . $matches[2] . "|";

$items = ["b" => 2, "a" => 1];
echo call_user_func_array("ksort", ["array" => &$items]) ? "K" : "k";
foreach ($items as $key => $value) {
    echo $key . $value;
}');
"#,
    );

    assert_eq!(out, "1:ab:a:b|Ka1b2");
}

/// Verifies eval first-class and Closure builtin callables preserve ref-like parameters.
#[test]
fn test_eval_ref_like_builtin_closures_write_back_aliases() {
    let out = compile_and_run(
        r#"<?php
eval('$sort = sort(...);
$items = [3, 1, 2];
echo $sort($items) ? "S" : "s";
echo implode(",", $items) . "|";

$settype = Closure::fromCallable("settype");
$value = "42";
echo $settype($value, "integer") ? "T" : "t";
echo gettype($value) . ":" . $value . "|";

$preg = preg_match(...);
$matches = [];
echo $preg("/(a)(b)/", "ab", $matches);
echo ":" . $matches[0] . ":" . $matches[1] . ":" . $matches[2] . "|";

$ksort = Closure::fromCallable("ksort");
$assoc = ["b" => 2, "a" => 1];
echo call_user_func_array($ksort, ["array" => &$assoc]) ? "K" : "k";
foreach ($assoc as $key => $entry) {
    echo $key . $entry;
}');
"#,
    );

    assert_eq!(out, "S1,2,3|Tinteger:42|1:ab:a:b|Ka1b2");
}

/// Verifies eval `call_user_func()` keeps ref-like builtin Closure args by value.
#[test]
fn test_eval_call_user_func_ref_like_builtin_closures_use_by_value_args() {
    let out = compile_and_run(
        r#"<?php
eval('$sort = sort(...);
$items = [3, 1, 2];
echo call_user_func($sort, $items) ? "S:" : "s:";
echo implode(",", $items) . "|";

$settype = Closure::fromCallable("settype");
$value = "42";
echo call_user_func($settype, $value, "integer") ? "T:" : "t:";
echo gettype($value) . ":" . $value . "|";

$preg = preg_match(...);
$matches = [];
echo call_user_func($preg, "/(a)(b)/", "ab", $matches);
echo ":" . count($matches) . "|";

$push = Closure::fromCallable("array_push");
$front = ["a"];
echo call_user_func($push, $front, "b") . ":" . implode(",", $front);');
"#,
    );

    assert_eq!(out, "S:3,1,2|T:string:42|1:0|2:a");
}

/// Verifies additional eval ref-like builtin callables write back through Closure dispatch.
#[test]
fn test_eval_ref_like_builtin_closures_write_back_extended_aliases() {
    let out = compile_and_run(
        r#"<?php
eval('$push = Closure::fromCallable("array_push");
$items = [1];
echo $push($items, 2, 3) . ":" . implode(",", $items) . "|";

$unshift = array_unshift(...);
$front = ["b"];
echo $unshift($front, "a") . ":" . implode(",", $front) . "|";

$splice = Closure::fromCallable("array_splice");
$letters = ["a", "b", "c", "d"];
$removed = call_user_func_array(
    $splice,
    ["array" => &$letters, "offset" => 1, "length" => 2, "replacement" => ["x", "y"]]
);
echo implode(",", $removed) . ":" . implode(",", $letters) . "|";

$walk = Closure::fromCallable("array_walk");
$walked = [1, 2];
$callback = function (&$value, $key) { $value = ($value * 10) + $key; };
echo $walk($walked, $callback) ? "W:" : "w:";
echo implode(",", $walked) . "|";

$pregAll = preg_match_all(...);
$matches = [];
echo $pregAll("/a(.)/", "ab ac", $matches);
echo ":" . implode(",", $matches[0]) . ":" . implode(",", $matches[1]);');
"#,
    );

    assert_eq!(out, "3:1,2,3|2:a,b|b,c:a,x,y,d|W:10,21|2:ab,ac:b,c");
}
