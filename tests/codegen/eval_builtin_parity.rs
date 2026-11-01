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
    compile_and_run, compile_and_run_capture_with_regex, compile_and_run_with_regex,
    elephc_cli_command_with_eval_catalog_native, fs, make_cli_test_dir, run_binary,
};

/// Verifies eval uses php-src byte-span and nullable-window semantics.
#[test]
fn test_eval_strcspn_and_strspn_php_src_semantics() {
    let out = compile_and_run(
        r#"<?php
echo eval('return strcspn("22222222aaaa bbb1111 cccc", "1234", 9, 6)
    . ":" . strspn("22222222aaaa bbb1111 cccc", "1234", 2, 3)
    . ":" . strcspn("abc123", "123", 1, null)
    . ":" . strspn("abc", "")
    . ":" . call_user_func_array("strcspn", ["abc123", "123"]);');
"#,
    );
    assert_eq!(out, "6:3:2:0:3");
}

/// Verifies `preg_quote()` gives eval and compiled code the same answer, delimiter included.
///
/// The interpreter owns a second copy of the escape table, so a divergence here means the two
/// tables have drifted apart rather than that either is merely incomplete.
#[test]
fn test_eval_preg_quote_matches_compiled() {
    let out = compile_and_run(
        r#"<?php
$subject = "a.b+c/d";
$compiled = preg_quote($subject) . "|" . preg_quote($subject, "/");
$evaluated = eval('return preg_quote("a.b+c/d") . "|" . preg_quote("a.b+c/d", "/");');
echo $compiled === $evaluated ? "same" : "DIFFER", "|", $evaluated;
"#,
    );
    assert_eq!(out, r#"same|a\.b\+c/d|a\.b\+c\/d"#);
}

/// Compiles and runs an eval fixture with both native regex providers explicitly enabled.
fn compile_and_run_with_mbstring_and_regex(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_eval_builtin_catalog");
    let php = dir.join("main.php");
    fs::write(&php, source).expect("write eval builtin catalog fixture");

    let compiled = elephc_cli_command_with_eval_catalog_native(&dir)
        .args(["--with-mbstring", "--with-regex"])
        .arg(&php)
        .output()
        .expect("compile eval builtin catalog fixture");
    assert!(
        compiled.status.success(),
        "eval builtin catalog fixture failed to compile:\n{}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    let executed = run_binary(&php.with_extension(""), &dir);
    assert!(
        executed.status.success(),
        "eval builtin catalog fixture failed to run:\n{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    let stdout = String::from_utf8(executed.stdout).expect("eval builtin output must be UTF-8");
    fs::remove_dir_all(&dir).expect("remove eval builtin catalog fixture");
    stdout
}

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

/// Verifies eval `function_exists()` sees every compiler builtin available on the host target.
#[test]
fn test_eval_function_exists_covers_static_builtin_catalog() {
    if crate::support::skip_without_gmp_native(
        "test_eval_function_exists_covers_static_builtin_catalog",
    ) {
        return;
    }
    let mut fragment = String::new();
    let target = elephc::codegen_support::platform::Target::detect_host();
    for name in elephc::builtin_metadata::php_visible_builtin_names_for_target(target) {
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
    let out = compile_and_run_with_mbstring_and_regex(&source);

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

/// Verifies eval's sort family takes php's `$flags` argument. Laravel's `LoadConfiguration` sorts
/// its config file map with `ksort($files, SORT_NATURAL)`, which eval refused outright.
#[test]
fn test_eval_sort_family_honours_sort_flags() {
    let out = compile_and_run(
        r#"<?php
eval('$files = ["view10" => 1, "app" => 2, "view2" => 3, "App" => 4];
ksort($files, SORT_NATURAL);
echo implode(",", array_keys($files)), "\n";
krsort($files, SORT_STRING);
echo implode(",", array_keys($files)), "\n";
$v = ["10", "9", "2", "1"];
sort($v, SORT_STRING);
echo implode(",", $v), "\n";
rsort($v, SORT_NUMERIC);
echo implode(",", $v), "\n";
$a = ["x" => "b10", "y" => "B2", "z" => "a1"];
asort($a, SORT_NATURAL | SORT_FLAG_CASE);
echo json_encode($a), "\n";
arsort($a, SORT_STRING);
echo json_encode($a), "\n";');
"#,
    );
    assert_eq!(
        out,
        "App,app,view2,view10\nview2,view10,app,App\n1,10,2,9\n10,9,2,1\n{\"z\":\"a1\",\"y\":\"B2\",\"x\":\"b10\"}\n{\"x\":\"b10\",\"z\":\"a1\",\"y\":\"B2\"}\n"
    );
}

/// Verifies eval hands a delimited pattern's body to PCRE2 verbatim, as php and compiled code do.
///
/// The interpreter used to strip the backslash from every escaped delimiter, which changes what an
/// escaped METACHARACTER means: `\(` inside a `(...)`-delimited pattern opened a group (the
/// phpdotenv lexer pattern stopped compiling) and `\+` inside `+...+` became a quantifier.
#[test]
fn test_eval_preg_keeps_escaped_delimiters_in_the_pattern() {
    let out = compile_and_run_with_regex(
        r#"<?php
$regex = '(([^(\s)]|\(|\)){1,10})A';
$plus = '+a\+b+';
echo preg_match($regex, 'f(x) y', $m), $m[0], ":", preg_match($plus, 'xa+b'), preg_match($plus, 'aab'), "\n";
eval('echo preg_match($regex, "f(x) y", $m), $m[0], ":", preg_match($plus, "xa+b"), preg_match($plus, "aab"), "\n";');
"#,
    );

    assert_eq!(out, "1f(x):10\n1f(x):10\n");
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

/// Verifies `headers_sent()` inside eval reports the COMPILED program's flag, not an eval-local one.
///
/// The flag lives in generated-runtime storage and is raised by the real-output path, so the only
/// honest answer comes from the compiled side. Before this it was not joined at all and every
/// interpreted caller died with "call to undefined function headers_sent()" -- which is what
/// Composer's own `vendor/composer/platform_check.php` hits the moment its version guard fails.
///
/// `php -n` 8.5.6 (`scratchpad/hsent.php`, this session): `clean` / `x` / `sent`.
#[test]
fn test_eval_headers_sent_reads_the_compiled_output_flag() {
    let out = compile_and_run(
        r#"<?php
echo eval("return headers_sent() ? \"sent\" : \"clean\";"), "\n";
echo "x\n";
echo eval("return headers_sent() ? \"sent\" : \"clean\";"), "\n";
"#,
    );

    assert_eq!(out, "clean\nx\nsent\n");
}

/// Verifies eval's `defined()` answers a `Class::CONST` name, compiled and eval-declared alike,
/// and says `false` for a missing class or constant instead of failing. Symfony's
/// `DebugHandlersListener::getSubscribedEvents()` gates a listener on
/// `defined('Symfony\Component\Console\ConsoleEvents::COMMAND')`.
#[test]
fn test_eval_defined_answers_class_constants() {
    let out = compile_and_run(
        r#"<?php
final class Compiled { public const X = 1; }
eval('final class Interpreted { public const Y = 2; }');
echo eval('return var_export([defined("Compiled::X"), defined("Interpreted::Y"), defined("Compiled::Z"), defined("Nope::X")], true);');
"#,
    );

    assert_eq!(out, "array (\n  0 => true,\n  1 => true,\n  2 => false,\n  3 => false,\n)");
}

/// Verifies php's builtin `Attribute` class: its constants compiled and interpreted, and an
/// interpreted attribute class whose `#[\Attribute(...)]` flags combine them stays reflectable.
/// Symfony's `Routing\Attribute\Route` has that shape and is only ever read by the interpreter.
#[test]
fn test_attribute_constants_and_interpreted_attribute_flags() {
    let out = compile_and_run(
        r#"<?php
echo \Attribute::TARGET_CLASS, ' ', \Attribute::TARGET_ALL, ' ', (new \Attribute())->flags, "\n";
eval('#[\Attribute(\Attribute::IS_REPEATABLE | \Attribute::TARGET_METHOD)] final class Flagged {}');
echo eval('$a = (new \ReflectionClass("Flagged"))->getAttributes()[0]; return $a->getName() . json_encode($a->getArguments());');
"#,
    );

    assert_eq!(out, "1 127 127\nAttribute[132]");
}

/// Verifies `$closure(...)` in eval hands back the same compiled closure, signature intact.
/// Symfony's `AttributeAutoconfigurationPass` reflects `new \ReflectionFunction($callable(...))`
/// and read arity 0 from the re-wrapped closure.
#[test]
fn test_eval_first_class_callable_of_a_closure_keeps_its_signature() {
    let out = compile_and_run(
        r#"<?php
$c = static function (int $a, string ...$rest): void {};
echo eval('$r = new \ReflectionFunction($c(...)); return $r->getNumberOfParameters() . ($c(...) === $c ? "same" : "copy");');
"#,
    );

    assert_eq!(out, "2same");
}

/// Verifies `mb_strtoupper()` / `mb_strtolower()` give eval and compiled code the same bytes.
///
/// Both backends call the same `elephc-iconv` engine, so a divergence here means the AOT
/// staging or the Magician binding changed an argument on its way in (a null encoding cast
/// to `""`, say) rather than a difference in the case tables.
#[test]
fn test_eval_mb_case_mapping_matches_compiled() {
    let out = compile_and_run(
        r#"<?php
$compiled = [
    mb_strtoupper("abc"),
    mb_strtoupper("straße éà ǆ ﬁ"),
    mb_strtolower("İSTANBUL ΣΑΣ"),
    mb_strtoupper("a\xffb"),
    mb_strtoupper("héllo", "utf8"),
    mb_strtoupper("\xe9t\xe9", "ISO-8859-1"),
    mb_strtolower("ÀB", null),
];
$evaluated = eval('return [
    mb_strtoupper("abc"),
    mb_strtoupper("straße éà ǆ ﬁ"),
    mb_strtolower("İSTANBUL ΣΑΣ"),
    mb_strtoupper("a\xffb"),
    mb_strtoupper("héllo", "utf8"),
    mb_strtoupper("\xe9t\xe9", "ISO-8859-1"),
    mb_strtolower("ÀB", null),
];');
foreach ($compiled as $index => $value) {
    echo $value === $evaluated[$index] ? "=" : "!", bin2hex($evaluated[$index]), "\n";
}
echo eval('try { mb_strtoupper("a", "nope"); } catch (ValueError $e) { return get_class($e) . ": " . $e->getMessage(); }'), "\n";
echo eval('return function_exists("mb_strtoupper") && function_exists("MB_STRTOLOWER") ? "visible" : "hidden";');
"#,
    );

    assert_eq!(
        out,
        "=414243\n\
=5354524153534520c389c38020c784204649\n\
=69cc877374616e62756c20cf83ceb1cf82\n\
=413f42\n\
=48c3894c4c4f\n\
=c954c9\n\
=c3a062\n\
ValueError: mb_strtoupper(): Argument #2 ($encoding) must be a valid encoding, \"nope\" given\n\
visible"
    );
}

/// Verifies interpreted `preg_*` accepts NUL bytes in the pattern and in the subject.
///
/// php hands PCRE2 both with their lengths. The interpreter built C strings from them, so a
/// pattern holding `"\0"` -- Laravel's `Str::trim()` puts one in its whitespace class -- failed as
/// an interpreter error, and a subject holding one silently never matched.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_eval_preg_takes_nul_bytes_in_pattern_and_subject() {
    let out = compile_and_run_with_regex(
        r#"<?php
$code = 'return json_encode([
    preg_replace("~[\\s\0]+~u", "", " x\0y "),
    preg_match("~a\0b~", "a\0b"),
    preg_match("~a\\\\\0b~", "a\0b"),
    preg_split("~\0~", "a\0b\0c"),
    preg_match("~c~", "a\0b\0c"),
]);';
echo eval($code), "\n";
echo preg_match("/x/", "x"), "\n";
"#,
    );
    assert_eq!(out, "[\"xy\",1,1,[\"a\",\"b\",\"c\"],1]\n1\n");
}

/// Builds a program that runs `body` natively, prints `|`, then runs the same `body` through a
/// runtime-selected `eval()` source, so the interpreter bridge (never literal AOT lowering)
/// executes the second half.
fn native_then_dynamic_eval(body: &str) -> String {
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php\n$code = $argc > 1 ? $argv[1] : '{quoted}';\n{body}\necho \"|\";\neval($code);\n")
}

/// Verifies the eval interpreter honours `in_array()`'s and `array_search()`'s `$strict`.
///
/// Both hooks took exactly two arguments, so any third argument failed the whole eval call
/// (#1472). Numeric strings separate the two comparisons: `"10" == "1e1"` but `"10" !== "1e1"`.
/// Only the eval half runs here, and it is checked against PHP 8.5 directly, because the
/// compiled `array_search()` has its own loose-comparison gap for numeric strings.
#[test]
fn test_eval_array_search_strict_parity() {
    let out = compile_and_run(
        r#"<?php
$code = $argc > 1 ? $argv[1] : 'echo in_array("10", ["1e1"]) ? "Y" : "N", in_array("10", ["1e1"], true) ? "Y" : "N";
echo in_array("10", ["1e1", "10"], true) ? "Y" : "N";
echo ":", array_search("10", ["a" => "1e1", "b" => "10"]), array_search("10", ["a" => "1e1", "b" => "10"], true);
echo ":", array_search("1e1", ["10"], true) === false ? "false" : "bad", ":", array_search("1e1", ["10"]);
echo ":", call_user_func("in_array", "10", ["1e1"], true) ? "Y" : "N";
echo call_user_func_array("array_search", ["10", ["1e1", "10"], true]);
echo in_array(needle: null, haystack: [0], strict: true) ? "Y" : "N";';
eval($code);
"#,
    );

    assert_eq!(out, "YNY:ab:false:0:N1N");
}

/// Verifies the eval interpreter renumbers `array_chunk()` string keys unless `$preserve_keys`
/// is set, and raises the compiled backend's catchable `ValueError` for a zero length.
///
/// Eval kept string keys when not preserving and failed the whole eval call for a
/// non-positive length (#1295). Both halves print PHP 8.5's output.
#[test]
fn test_eval_array_chunk_key_and_length_parity() {
    let body = r#"foreach (array_chunk(["a" => 1, "b" => 2, "c" => 3], 2) as $chunk) { echo json_encode($chunk); }
foreach (array_chunk([5 => "x", "k" => "y", 9 => "z"], 2, true) as $chunk) { echo json_encode($chunk); }
try { array_chunk([1], 0); } catch (ValueError $e) { echo ":", $e->getMessage(); }"#;
    let out = compile_and_run(&native_then_dynamic_eval(body));

    let expected = r#"[1,2][3]{"5":"x","k":"y"}{"9":"z"}:array_chunk(): Argument #2 ($length) must be greater than 0"#;
    assert_eq!(out, format!("{expected}|{expected}"));
}

/// Verifies the eval interpreter honours `htmlspecialchars()`'s and `htmlentities()`'s `$flags`
/// exactly as the compiled helper does.
///
/// Eval ignored `$flags` and always escaped both quotes (#1464): `ENT_NOQUOTES` and
/// `ENT_COMPAT` output differed from the compiled program, and the `&apos;` doctypes rendered
/// `&#039;`. Both halves print PHP 8.5's output.
#[test]
fn test_eval_html_entity_flags_parity() {
    let body = r#"$s = "<p class=\"q\">It's</p>";
echo htmlspecialchars($s, ENT_NOQUOTES), ";", htmlspecialchars($s, ENT_COMPAT), ";";
echo htmlspecialchars($s, ENT_QUOTES | ENT_HTML5), ";", htmlentities($s, ENT_QUOTES | ENT_XML1), ";";
echo htmlspecialchars($s), ";", htmlentities($s, ENT_NOQUOTES, "UTF-8");"#;
    let out = compile_and_run(&native_then_dynamic_eval(body));

    let expected = concat!(
        "&lt;p class=\"q\"&gt;It's&lt;/p&gt;;",
        "&lt;p class=&quot;q&quot;&gt;It's&lt;/p&gt;;",
        "&lt;p class=&quot;q&quot;&gt;It&apos;s&lt;/p&gt;;",
        "&lt;p class=&quot;q&quot;&gt;It&apos;s&lt;/p&gt;;",
        "&lt;p class=&quot;q&quot;&gt;It&#039;s&lt;/p&gt;;",
        "&lt;p class=\"q\"&gt;It's&lt;/p&gt;",
    );
    assert_eq!(out, format!("{expected}|{expected}"));
}

/// Verifies the eval interpreter packs and renders IPv6 addresses like the compiled
/// `inet_pton()`/`inet_ntop()` helpers, and refuses the same invalid shapes.
///
/// Eval was IPv4-only, so every IPv6 address packed to `false` (#1157). `01.2.3.4` pins that
/// the platform parser, not eval's old `ip2long()` scanner, now decides validity, so eval
/// answers whatever the compiled helper answers on that platform. That answer follows the
/// platform's `inet_pton(3)`, as PHP's does: glibc refuses the leading-zero octet, while
/// Apple's BSD parser accepts it as `1.2.3.4`. The two halves must match each other on every
/// platform, and each prints PHP 8.5's output for the host platform.
#[test]
fn test_eval_inet_ipv6_parity() {
    let body = r#"foreach (["::1", "2001:db8:85a3:0:0:8a2e:370:7334", "::ffff:192.0.2.128", "10.0.0.1", "gggg::1", "01.2.3.4"] as $a) {
    $p = inet_pton($a);
    echo $p === false ? "false" : bin2hex($p) . "=" . inet_ntop($p), ";";
}
echo inet_ntop("123456789012345") === false ? "F" : "bad";"#;
    let out = compile_and_run(&native_then_dynamic_eval(body));

    let (native, evaluated) = out.split_once('|').expect("both halves print their results");
    assert_eq!(evaluated, native, "eval must answer exactly what the compiled helpers answer");
    let leading_zero_octet = if cfg!(any(target_os = "macos", target_os = "ios")) {
        "01020304=1.2.3.4;"
    } else {
        "false;"
    };
    let expected = format!(
        concat!(
            "00000000000000000000000000000001=::1;",
            "20010db885a3000000008a2e03707334=2001:db8:85a3::8a2e:370:7334;",
            "00000000000000000000ffffc0000280=::ffff:192.0.2.128;",
            "0a000001=10.0.0.1;false;{}F",
        ),
        leading_zero_octet
    );
    assert_eq!(native, expected);
}

/// Verifies eval `putenv()` reports libc's status the way the compiled `putenv()` does.
///
/// Eval returned a hard-coded `true` from Rust's environment setters (#911), and those setters
/// panicked on a NUL byte, killing the program. libc reads the assignment up to that NUL, so
/// `NAME=value\0tail` stores `value` on both sides, as PHP does. A leading NUL leaves an empty
/// name that `unsetenv(3)` refuses; the two halves must report the same status for it.
#[test]
fn test_eval_putenv_libc_status_parity() {
    let out = compile_and_run(
        r#"<?php
$code = $argc > 1 ? $argv[1] : 'echo putenv("ELEPHC_EVAL_PUTENV_PARITY=eval\0tail") ? "set:" : "bad:", getenv("ELEPHC_EVAL_PUTENV_PARITY");
return putenv("\0ELEPHC_EVAL_PUTENV_PARITY");';
echo putenv("ELEPHC_EVAL_PUTENV_PARITY=aot\0tail") ? "set:" : "bad:", getenv("ELEPHC_EVAL_PUTENV_PARITY");
$native = putenv("\0ELEPHC_EVAL_PUTENV_PARITY");
echo "|";
$evaluated = eval($code);
echo "|", $native === $evaluated ? "same" : "differ";
"#,
    );

    assert_eq!(out, "set:aot|set:eval|same");
}
