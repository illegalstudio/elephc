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

use crate::support::{
    compile_and_run, compile_and_run_capture, compile_and_run_capture_with_regex,
    compile_and_run_in_dir, compile_and_run_with_regex,
};

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
        let contract = elephc_builtin_contract::lookup(name)
            .expect("compiler-visible builtin must have a shared contract");
        if !matches!(
            elephc_builtin_contract::eval_support(contract),
            elephc_builtin_contract::BackendSupport::Implemented(
                elephc_builtin_contract::BackendImplementation::Registry
            )
        ) {
            continue;
        }
        writeln!(
            &mut fragment,
            "if (!function_exists(\"{name}\")) {{ echo \"{name},\"; }}"
        )
        .expect("write eval builtin probe");
    }
    fragment.push_str("return \"ok\";");

    let source = format!("<?php\necho eval({});\n", php_single_quoted_literal(&fragment));
    let out = compile_and_run_with_regex(&source);

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

/// Verifies dynamic eval links and dispatches Magician's encoding-aware `mb_strlen()` path.
#[test]
fn test_eval_mb_strlen_encoding_parity() {
    let out = compile_and_run(
        r#"<?php
$source = 'echo mb_strlen("héllo", "8bit");
echo ":";
$utf16 = chr(104) . chr(0) . chr(233) . chr(0);
echo mb_strlen($utf16, "UTF-16LE");';
eval($source);
"#,
    );

    assert_eq!(out, "6:2");
}

/// Verifies eval preg builtins use PCRE2 features that Rust regex did not support.
#[test]
fn test_eval_preg_uses_pcre2_lookaround_semantics() {
    let out = compile_and_run_with_regex(
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
    let out = compile_and_run_with_regex(
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
    let out = compile_and_run_with_regex(
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
    let out = compile_and_run_with_regex(
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

/// Verifies eval `call_user_func_array()` keeps non-reference builtin Closure args by value.
#[test]
fn test_eval_call_user_func_array_ref_like_builtin_closures_keep_non_ref_args_by_value() {
    let out = compile_and_run_with_regex(
        r#"<?php
eval('$sort = sort(...);
$items = [3, 1, 2];
$sortArgs = [$items];
echo call_user_func_array($sort, $sortArgs) ? "S:" : "s:";
echo implode(",", $items) . ":" . implode(",", $sortArgs[0]) . "|";

$settype = Closure::fromCallable("settype");
$value = "42";
$setArgs = [$value, "integer"];
echo call_user_func_array($settype, $setArgs) ? "T:" : "t:";
echo gettype($value) . ":" . $value . ":" . gettype($setArgs[0]) . ":" . $setArgs[0] . "|";

$preg = preg_match(...);
$matches = [];
$pregArgs = ["/(a)(b)/", "ab", $matches];
echo call_user_func_array($preg, $pregArgs);
echo ":" . count($matches) . ":" . count($pregArgs[2]) . "|";

$push = Closure::fromCallable("array_push");
$front = ["a"];
$pushArgs = [$front, "b"];
echo call_user_func_array($push, $pushArgs) . ":" .
    implode(",", $front) . ":" . implode(",", $pushArgs[0]);');
"#,
    );

    assert_eq!(out, "S:3,1,2:3,1,2|T:string:42:string:42|1:0:0|2:a:a");
}

/// Verifies additional eval ref-like builtin callables write back through Closure dispatch.
#[test]
fn test_eval_ref_like_builtin_closures_write_back_extended_aliases() {
    let out = compile_and_run_with_regex(
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

/// Verifies ref-like builtin callbacks preserve writeback through AOT callable parameters.
#[test]
fn test_eval_ref_like_builtin_callables_pass_to_aot_callable_params() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
class EvalRefLikeBuiltinCallableBridge {
    public string $value = "";

    public function __construct(callable $callback, string $label) {
        $items = [3, "1", 2];
        $ok = $callback($items);
        $this->value = $label . ":" . ($ok ? "T" : "F") . ":" . implode(",", $items);
    }

    public function convert(callable $callback, string $label): string {
        $value = $label === "never" ? "x" : 42;
        $ok = $callback($value, "string");
        return $label . ":" . ($ok ? "T" : "F") . ":" . gettype($value) . ":" . $value;
    }

    public static function match(callable $callback, string $label): string {
        $matches = [0, ""];
        $count = $callback("/(a)(b)/", "ab", $matches);
        return $label . ":" . $count . ":" . implode(":", $matches);
    }

    public function push(callable $callback, string $label): string {
        $items = $label === "never" ? [0] : ["a"];
        $count = $callback($items, "b");
        return $label . ":" . $count . ":" . implode(",", $items);
    }
}

echo eval('$out = [];
$box = new EvalRefLikeBuiltinCallableBridge("sort", "s");
$out[] = $box->value;
$box = new EvalRefLikeBuiltinCallableBridge(sort(...), "f");
$out[] = $box->value;
$box = new EvalRefLikeBuiltinCallableBridge(Closure::fromCallable("sort"), "c");
$out[] = $box->value;

$box = new EvalRefLikeBuiltinCallableBridge("sort", "seed");
$out[] = $box->convert("settype", "s");
$out[] = $box->convert(settype(...), "f");
$out[] = $box->convert(Closure::fromCallable("settype"), "c");

$out[] = EvalRefLikeBuiltinCallableBridge::match("preg_match", "s");
$out[] = EvalRefLikeBuiltinCallableBridge::match(preg_match(...), "f");
$out[] = EvalRefLikeBuiltinCallableBridge::match(Closure::fromCallable("preg_match"), "c");

$out[] = $box->push("array_push", "s");
$out[] = $box->push(array_push(...), "f");
$out[] = $box->push(Closure::fromCallable("array_push"), "c");

return implode("|", $out);');
"#,
    );

    assert!(
        out.success,
        "stdout:\n{}\nstderr:\n{}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "s:T:1,2,3|f:T:1,2,3|c:T:1,2,3|\
s:T:string:42|f:T:string:42|c:T:string:42|\
s:1:ab:a:b|f:1:ab:a:b|c:1:ab:a:b|\
s:2:a,b|f:2:a,b|c:2:a,b"
    );
}

/// Verifies the eval interpreter reproduces every `str_word_count()` result shape, including
/// the byte-offset map, the extra `$characters` alphabet, and php-src's boundary trims.
#[test]
fn test_eval_str_word_count_parity() {
    let out = compile_and_run(
        r#"<?php
eval('echo str_word_count("Hello friend, you\'re here"), ":";
echo implode(",", str_word_count("Hello friend", 1)), ":";
foreach (str_word_count("one two", 2) as $offset => $word) { echo $offset, "=", $word, ";"; }
echo implode(",", str_word_count("fri3nd", 1, "3")), ":";
echo implode(",", str_word_count("-abc-", 1));');
"#,
    );

    assert_eq!(out, "4:Hello,friend:0=one;4=two;fri3nd:abc");
}

/// Verifies the eval interpreter reproduces every `count_chars()` mode and raises php-src's
/// catchable `ValueError` for an unknown one.
#[test]
fn test_eval_count_chars_parity() {
    let out = compile_and_run(
        r#"<?php
eval('$used = count_chars("hello", 1);
foreach ($used as $byte => $count) { echo $byte, "=", $count, ";"; }
echo ":", count_chars("hello world", 3), ":", strlen(count_chars("hello world", 4)), ":", count(count_chars("aab", 0));
try { count_chars("ab", 7); } catch (\ValueError $e) { echo ":", $e->getMessage(); }');
"#,
    );

    assert_eq!(
        out,
        "101=1;104=1;108=2;111=1;: dehlorw:248:256:count_chars(): Argument #2 ($mode) must be between 0 and 4 (inclusive)"
    );
}

/// Verifies the eval interpreter reproduces both `strtr()` shapes, including longest-match-first
/// selection, integer keys, and keys longer than the subject.
#[test]
fn test_eval_strtr_parity() {
    let out = compile_and_run(
        r#"<?php
eval('echo strtr("foo bar", ["foo"=>"bar","bar"=>"baz"]), ":";
echo strtr("abc", ["a"=>"b","ab"=>"X"]), ":";
echo strtr("12345", [1=>"one", 23=>"two-three"]), ":";
echo strtr("abcd", "abc", "xy"), ":";
echo strtr("abc", ["abcd"=>"X"]);');
"#,
    );

    assert_eq!(out, "bar baz:Xc:onetwo-three45:xycd:abc");
}

/// Verifies the eval interpreter raises php-src's catchable `ValueError` for an unknown
/// `str_word_count()` format, matching the compiled backend's guard.
#[test]
fn test_eval_str_word_count_invalid_format_parity() {
    let out = compile_and_run(
        r#"<?php
eval('try { str_word_count("ab", 5); } catch (\ValueError $e) { echo $e->getMessage(); }');
"#,
    );

    assert_eq!(
        out,
        "str_word_count(): Argument #2 ($format) must be a valid format value"
    );
}

/// Verifies the eval interpreter reproduces `file_get_contents()`'s `$offset`/`$length` window,
/// its unreachable-seek `false`, and its negative-`$length` `ValueError`, matching what the
/// compiled backend produces for the same reads.
///
/// Both sides now declare the same five-parameter PHP 8.4 signature, so this fixture also pins
/// that the eval dispatcher accepts every argument position the static catalog advertises.
#[test]
fn test_eval_file_get_contents_offset_length_parity() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("eval_fgc.txt", "ABCDEFGHIJ");
echo file_get_contents("eval_fgc.txt", false, null, 3, 4), ":";
eval('echo file_get_contents("eval_fgc.txt", false, null, 3, 4), ":";
echo file_get_contents("eval_fgc.txt", false, null, -3), ":";
echo file_get_contents("eval_fgc.txt", false, null, 20) === "" ? "past-eof" : "bad", ":";
echo file_get_contents("eval_fgc.txt", true, null, 4, 3), ":";
try { file_get_contents("eval_fgc.txt", false, null, 0, -1); } catch (\ValueError $e) { echo $e->getMessage(); }');
unlink("eval_fgc.txt");
"#,
    );

    assert_eq!(
        out,
        "DEFG:DEFG:HIJ:past-eof:EFG:file_get_contents(): Argument #5 ($length) must be greater than or equal to 0"
    );
}

/// Verifies the eval interpreter and the compiled backend agree on `array_splice()`'s
/// `$replacement`: the same removed slice and the same mutated receiver on both sides.
#[test]
fn test_eval_array_splice_replacement_parity() {
    let out = compile_and_run(
        r#"<?php
$a = [1,2,3,4,5];
$removed = array_splice($a, 1, 2, [90,91,92]);
echo implode(",", $a), "|", implode(",", $removed), ":";
eval('$b = [1,2,3,4,5];
$removed2 = array_splice($b, 1, 2, [90,91,92]);
echo implode(",", $b), "|", implode(",", $removed2), ":";
$c = [1,2,3];
echo count(array_splice($c, 1, 0, [7,8])), "|", implode(",", $c), ":";
$d = [1,2,3];
array_splice($d, 1, 1, 9);
echo implode(",", $d);');
"#,
    );

    assert_eq!(
        out,
        "1,90,91,92,4,5|2,3:1,90,91,92,4,5|2,3:0|1,7,8,2,3:1,9,3"
    );
}

/// Verifies `glob()`'s flags behave the same inside `eval()` as they do compiled.
///
/// The eval interpreter walks the filesystem itself rather than calling libc `glob()`, so this is
/// a genuinely second implementation of php's behaviour. Every expectation was read off `php -n`
/// 8.5.6 first, including the two that are not the obvious answers: with `GLOB_BRACE` the sort
/// runs per expansion — `{c*,a*}` answers `c.log` before `a.txt` — and duplicates are kept.
#[test]
fn test_eval_glob_flags_match_the_compiled_surface() {
    let out = compile_and_run(
        r#"<?php
mkdir("eg");
mkdir("eg/sub");
file_put_contents("eg/a.txt", "a");
file_put_contents("eg/b.md", "b");
file_put_contents("eg/c.log", "c");

eval('echo implode(",", glob("eg/*", GLOB_ONLYDIR)), "\n";');
eval('echo implode(",", glob("eg/*", GLOB_MARK)), "\n";');
eval('echo implode(",", glob("eg/*", GLOB_MARK | GLOB_ONLYDIR)), "\n";');
eval('echo implode(",", glob("eg/zz*", GLOB_NOCHECK)), "\n";');
eval('echo count(glob("eg/zz*", GLOB_NOCHECK | GLOB_ONLYDIR)), "\n";');
// The sort is per brace expansion, and the expansions keep the order they were written in.
eval('echo implode(",", glob("eg/{c*,a*}", GLOB_BRACE)), "\n";');
// Overlapping alternatives list the same match twice; php does not deduplicate.
eval('echo implode(",", glob("eg/{*.txt,*}", GLOB_BRACE)), "\n";');
eval('echo implode(",", glob("eg/{a.txt,{b.md,c.log}}", GLOB_BRACE)), "\n";');
eval('echo implode(",", glob("eg/{zz,a.txt}", GLOB_BRACE | GLOB_NOCHECK)), "\n";');
eval('echo count(glob("eg/*", GLOB_NOSORT)), "\n";');

unlink("eg/a.txt");
unlink("eg/b.md");
unlink("eg/c.log");
rmdir("eg/sub");
rmdir("eg");
"#,
    );

    assert_eq!(
        out,
        "eg/sub\n\
         eg/a.txt,eg/b.md,eg/c.log,eg/sub/\n\
         eg/sub/\n\
         eg/zz*\n\
         0\n\
         eg/c.log,eg/a.txt\n\
         eg/a.txt,eg/a.txt,eg/b.md,eg/c.log,eg/sub\n\
         eg/a.txt,eg/b.md,eg/c.log\n\
         eg/zz,eg/a.txt\n\
         4\n"
    );
}

/// Verifies `eval()` refuses a glob flag php does not expose, exactly as the compiled path does.
///
/// 1024 is glibc's own `GLOB_BRACE`; php answers `false` to it on every platform because it is not
/// one of php's bits. The two engines read those bits from the same table, so they cannot come to
/// disagree about which values are valid.
#[test]
fn test_eval_glob_refuses_a_flag_php_does_not_expose() {
    let out = compile_and_run_capture(
        r#"<?php
mkdir("er");
file_put_contents("er/a.txt", "a");
eval('var_dump(glob("er/*", 1024));');
eval('var_dump(glob("er/*", -1));');
eval('var_dump(count(glob("er/*", GLOB_AVAILABLE_FLAGS)));');
unlink("er/a.txt");
rmdir("er");
"#,
    );

    assert!(out.success);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\nint(0)\n");
    let expected = "Warning: glob(): At least one of the passed flags is invalid or not \
                    supported on this platform\n";
    assert_eq!(out.diagnostics, expected.repeat(2));
}

/// Verifies a builtin the program names can be called through a variable.
///
/// Only 36 of 577 builtins could before: `$fn = "feof"; $fn($h);` was `Fatal error: Call to
/// undefined function <dynamic>()` where php simply calls it. The 488 others shared one blanket
/// refusal — "typed backend operation has no runtime-selected wrapper contract" — which is a
/// policy default rather than a finding about any of them.
///
/// The resource argument is the point of the stream names here: the refusal was documented as
/// covering resources, and it does not.
#[test]
fn test_a_named_builtin_is_callable_through_a_variable() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("dc.txt", "abc");
$h = fopen("dc.txt", "r");
$fn = "feof";
echo var_export($fn($h), true), "|";
foreach (["ftell", "feof"] as $each) {
    echo var_export($each($h), true), "|";
}
$upper = "strtoupper";
echo $upper("ab"), "|";
$pick = time() > 0 ? "strrev" : "trim";
echo $pick("abc");
fclose($h);
unlink("dc.txt");
"#,
    );
    assert_eq!(out, "false|0|false|AB|cba");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a builtin NAME is a valid callback for the higher-order array builtins.
///
/// `array_map("strtoupper", $a)` was `Undefined function: strtoupper` — a checker diagnostic — as
/// were `array_filter` and `usort` with any builtin name, while `call_user_func("strtoupper", …)`
/// worked. One lookup was the difference: the callback branch resolved through
/// `check_function_call`, which knows USER functions only.
#[test]
fn test_a_builtin_name_is_a_valid_array_callback() {
    let out = compile_and_run(
        r#"<?php
print_r(array_map("strtoupper", ["a", "b"]));
$words = ["pear", "fig", "apple"];
usort($words, "strcmp");
print_r($words);
echo call_user_func("strrev", "abc"), "\n";
"#,
    );
    assert_eq!(
        out,
        "Array\n(\n    [0] => A\n    [1] => B\n)\n\
         Array\n(\n    [0] => apple\n    [1] => fig\n    [2] => pear\n)\n\
         cba\n"
    );
}

/// Verifies dynamic eval reaches Magician's iconv conversion and character-count paths.
#[test]
fn test_eval_iconv_conversion_parity() {
    let out = compile_and_run(
        r#"<?php
eval('echo bin2hex(iconv("UTF-8", "ISO-8859-1", "café"));
echo ":";
echo iconv_strlen("héllo");
echo ":";
echo iconv_substr("héllo", 1, 3);
echo ":";
echo iconv_strpos("héllo", "l");
echo ":";
echo iconv_strrpos("abcabc", "bc");');
"#,
    );

    assert_eq!(out, "636166e9:5:éll:2:4");
}

/// Verifies dynamic eval reaches Magician's MIME encoder, decoder, and header decoder.
#[test]
fn test_eval_iconv_mime_parity() {
    let out = compile_and_run(
        r#"<?php
eval('echo iconv_mime_encode("Subject", "Prüfung", ["scheme" => "Q"]);
echo "|";
echo iconv_mime_decode("Subject: =?ISO-8859-1?Q?Pr=FCfung?=");
echo "|";
$headers = iconv_mime_decode_headers("A: 1
B: 2
A: 3");
echo $headers["A"][0], $headers["A"][1], $headers["B"];');
"#,
    );

    assert_eq!(
        out,
        "Subject: =?UTF-8?Q?Pr=C3=BCfung?=|Subject: Prüfung|132"
    );
}

/// Verifies eval shares the process-wide encoding trio with the native builtins.
#[test]
fn test_eval_iconv_encoding_state_is_shared() {
    let out = compile_and_run(
        r#"<?php
iconv_set_encoding("internal_encoding", "ISO-8859-1");
eval('echo iconv_get_encoding("internal_encoding");
echo ":";
echo iconv_strlen("héllo");
echo ":";
echo iconv_set_encoding("internal_encoding", "UTF-8") ? "yes" : "no";');
echo ":", iconv_strlen("héllo");
"#,
    );

    assert_eq!(out, "ISO-8859-1:6:yes:5");
}

/// Verifies eval raises the same catchable `ValueError` as the compiled backend.
#[test]
fn test_eval_iconv_strpos_offset_error_parity() {
    let out = compile_and_run(
        r#"<?php
try {
    eval('iconv_strpos("héllo", "l", 99);');
} catch (\ValueError $error) {
    echo get_class($error), "|", $error->getMessage();
}
"#,
    );

    assert_eq!(
        out,
        "ValueError|iconv_strpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
    );
}
