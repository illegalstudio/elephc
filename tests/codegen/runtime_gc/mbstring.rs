//! Purpose:
//! Checks ownership across mbstring's Rust buffers, eval casts, and PHP exceptions.
//!
//! Called from:
//! - The codegen test binary's runtime GC module.
//!
//! Key details:
//! - Repeated calls must not retain additional runtime strings or throwable objects.
//! - The dynamic source depends on argc and therefore executes through Magician.
//! - Reused input cells isolate bridge ownership from unrelated eval expression temporaries.

use crate::support::*;

/// Keeps changed and unchanged ASCII transforms balanced for large native and eval-owned strings.
#[test]
fn test_mbstring_ascii_case_origin_ownership() {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 12] {
            let calls = "$a = strtoupper($subject); $b = strtolower($a);".repeat(count);
            // Print once so residual ownership measures transforms independently of eval echo temporaries.
            let body = format!("echo bin2hex(strtolower(\"\\xffAZ\")), \"|\"; $subject = str_repeat(\"ASCII\", 12000); {calls} echo mb_strlen($b), \"|\";");
            let source = if eval {
                format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);")
            } else { format!("<?php {body}") };
            let output = compile_and_run_with_gc_stats(&source);
            assert!(output.success, "{}", output.stderr);
            assert_eq!(output.stdout, "ff617a|60000|");
            let (allocated, freed) = parse_gc_stats(&output.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "ASCII case conversion retained owners with eval={eval}");
    }
}

/// Checks call-array reference ownership at both original repetition counts.
fn check_mbstring_eval_call_array_reference_ownership(failing: bool) {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let body = if failing { "mb_strlen($arg, \"invalid-encoding\");" } else { "return 1;" };
        let call = if failing {
            "try { call_user_func_array($callback, $arguments); } catch (ValueError) {}\n"
        } else {
            "call_user_func_array($callback, $arguments);\n"
        };
        let calls = call.repeat(count);
        let source = format!(r#"<?php
$source = $argc > 0 ? '
function reference_callback(&$arg) {{ {body} }}
$callback = "reference_callback";
$value = "source"; $arguments = [&$value];
{calls}
unset($value); $copy = $arguments; $copy[0] = "changed";
echo json_encode(mb_convert_encoding($arguments, "UTF-8", "UTF-8"));
' : '';
eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, r#"["source"]"#);
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "call-array owners survived callback cleanup, failing={failing}");
}

/// Releases call-array reference owners after successful eval callbacks.
#[test]
fn test_mbstring_eval_call_array_reference_ownership_success() {
    check_mbstring_eval_call_array_reference_ownership(false);
}

/// Releases call-array reference owners after throwing eval callbacks.
#[test]
fn test_mbstring_eval_call_array_reference_ownership_failure() {
    check_mbstring_eval_call_array_reference_ownership(true);
}

/// Native typed callbacks write through persistent references and leave orphan array copies independent.
#[test]
fn test_mbstring_eval_call_array_reference_coercion() {
    let source = r#"<?php
function native_reference_coercion(int &$value): int { $value += 1; return $value; }
$source = $argc > 0 ? '
$value = "3"; $arguments = [&$value];
echo call_user_func_array("native_reference_coercion", $arguments), ":", gettype($value), ":";
unset($value); $copy = $arguments; $copy[0] = 9;
echo json_encode(mb_convert_encoding($arguments, "UTF-8", "UTF-8")), ":";
$value = "6"; $arguments = ["value" => &$value];
echo call_user_func_array("native_reference_coercion", $arguments), ":", gettype($value), ":";
unset($value); $copy = $arguments; $copy["value"] = 9;
echo json_encode(mb_convert_encoding($arguments, "UTF-8", "UTF-8"));
' : '';
eval($source);
"#;
    assert_eq!(compile_and_run(source), r#"4:integer:[4]:7:integer:{"value":7}"#);
}

/// Reflection invocation and construction keep original references through typed writeback.
#[test]
fn test_mbstring_eval_reflection_array_references() {
    let source = r#"<?php
$source = $argc > 0 ? '
function reflected_reference(int &$value): int { $value += 1; return $value; }
class ReflectedReferenceBox {
    public function __construct(int &$value) { $value += 1; }
    public function change(int &$value): int { $value += 1; return $value; }
}
$value = "3"; $arguments = [&$value];
$function = new ReflectionFunction("reflected_reference");
echo $function->invokeArgs($arguments), ":", gettype($value), ":";
unset($value); $copy = $arguments; $copy[0] = 9;
echo json_encode(mb_convert_encoding($arguments, "UTF-8", "UTF-8")), ":";
$value = "6"; $arguments = ["value" => &$value];
$class = new ReflectionClass("ReflectedReferenceBox");
$box = $class->newInstanceArgs($arguments);
$method = new ReflectionMethod("ReflectedReferenceBox", "change");
echo $method->invokeArgs($box, $arguments), ":", gettype($value), ":";
unset($value); $copy = $arguments; $copy["value"] = 9;
echo json_encode(mb_convert_encoding($arguments, "UTF-8", "UTF-8"));
' : '';
eval($source);
"#;
    assert_eq!(compile_and_run(source), r#"4:integer:[4]:8:integer:{"value":8}"#);
}

/// Iterator callbacks share references across visits and release their owners when traversal ends.
#[test]
fn test_mbstring_eval_iterator_array_references() {
    let source = r#"<?php
class ReferenceRange implements Iterator {
    private int $position;
    public function __construct() { $this->position = 0; }
    public function rewind(): void { $this->position = 0; }
    public function valid(): bool { return $this->position < 2; }
    public function current(): int { return $this->position; }
    public function key(): int { return $this->position; }
    public function next(): void { $this->position = $this->position + 1; }
}
$source = $argc > 0 ? '
function iterator_reference(&$value): bool { $value = $value . "!"; return true; }
$value = "source"; $arguments = [&$value];
echo iterator_apply(new ReferenceRange(), "iterator_reference", $arguments), ":";
unset($value); $copy = $arguments; $copy[0] = "changed";
echo json_encode(mb_convert_encoding($arguments, "UTF-8", "UTF-8"));
' : '';
eval($source);
"#;
    assert_eq!(compile_and_run(source), r#"2:["source!!"]"#);
}

/// Repeated by-value calls release parameter snapshots detached from persistent references.
#[test]
fn test_mbstring_eval_reference_parameter_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "read_arg($value);\n".repeat(count);
        let source = format!(r#"<?php
$source = $argc > 0 ? '
function read_arg($arg): int {{ return 1; }}
$value = "source";
$input = [&$value];
{calls}
echo "done";
' : '';
eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "parameter snapshots retained additional runtime owners");
}

/// Local array elements release their copied parameters after an ignored array return.
#[test]
fn test_mbstring_eval_reference_parameter_array_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "make_array($value);\n".repeat(count);
        let source = format!(r#"<?php
$source = $argc > 0 ? '
function make_array($arg): array {{ $result = [$arg]; return $result; }}
$value = "source"; $input = [&$value];
{calls}
echo "done";
' : '';
eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "returned arrays retained local parameter copies");
}

/// A detached object parameter is destroyed at function exit after the caller reference is cleared.
#[test]
fn test_mbstring_eval_reference_parameter_destructor() {
    let source = r#"<?php
$source = $argc > 0 ? '
class ScopeLife { public function __destruct() { echo "drop:"; } }
function use_arg($arg, &$current): int { $current = null; echo "body:"; return 1; }
$value = new ScopeLife(); $input = [&$value];
use_arg($value, $value); echo "after";
' : '';
eval($source);
"#;
    assert_eq!(compile_and_run(source), "body:drop:after");
}

/// Returned parameter values and by-reference writes survive releasing the function scope.
#[test]
fn test_mbstring_eval_reference_parameter_escape() {
    let source = r#"<?php
$source = $argc > 0 ? '
class ScopeHolder { public static $saved; }
function publish_arg($arg): int { ScopeHolder::$saved = $arg; return 1; }
function return_arg($arg) { return $arg; }
function coalesce_arg($arg) { return $arg ?? "fallback"; }
function ternary_arg($arg) { return $arg ? $arg : "fallback"; }
function short_arg($arg) { return $arg ?: "fallback"; }
function match_arg($arg) { return match (1) { 1 => $arg }; }
function export_arg($arg, &$out): int { $out = $arg; return 1; }
function finally_arg($arg) { try { return $arg; } finally { $arg = "changed"; } }
$value = "source"; $input = [&$value];
$output = [return_arg($value), coalesce_arg($value), ternary_arg($value), short_arg($value), match_arg($value), finally_arg($value)];
$out = "before"; export_arg($value, $out); $output[] = $out;
publish_arg($value);
$value = "after";
$output[] = ScopeHolder::$saved;
echo json_encode(mb_convert_encoding($output, "UTF-8", "UTF-8"));
' : '';
eval($source);
"#;
    assert_eq!(compile_and_run(source), r#"["source","source","source","source","source","source","source","source"]"#);
}

/// Repeated reference replacement preserves a borrowed source and releases both replaced owners.
#[test]
fn test_mbstring_eval_reference_replacement_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "$value = $replacement;\n".repeat(count);
        let source = format!(r#"<?php
$source = $argc > 0 ? '
$value = "before";
$replacement = "after";
$input = [&$value];
{calls}
echo $replacement, ":", json_encode(mb_convert_encoding($input, "UTF-8", "UTF-8"));
' : '';
eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "after:[\"after\"]");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "reference replacement retained additional runtime owners");
}

/// Verifies repeated eval mb_strlen failures release owned messages, throwables, and casts.
#[test]
fn test_mb_strlen_eval_exception_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "mb_strlen($subject); try { mb_strlen($subject, $encoding); } catch (ValueError) {}\n".repeat(count);
        let source = format!(r#"<?php
$source = $argc > 0 ? '
$subject = "日本語";
$encoding = "invalid-encoding";
{calls}
echo "done";
' : '';
eval($source);
"#);
        let out = compile_and_run_with_gc_stats(&source);
        assert!(out.success, "{}", out.stderr);
        assert_eq!(out.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&out.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "repeated mbstring calls retained runtime allocations");
}

/// Verifies native string results never retain their source or temporary bridge buffers.
#[test]
fn test_mbstring_native_string_result_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "mb_strlen($subject, nullable_encoding($start)); mb_strtoupper($subject); mb_strtolower($subject); mb_convert_case($subject, $mode); mb_ucfirst($subject); mb_lcfirst($subject); mb_strimwidth($subject, $start, $width, $marker);\n".repeat(count);
        let source = format!(r#"<?php
function nullable_encoding(int $start): ?string {{
    if ($start > 0) {{ return "UTF-8"; }}
    return null;
}}
$subject = $argc > 0 ? "Straße 東京" : "other";
$mode = MB_CASE_FOLD;
$start = 0;
$width = 7;
$marker = "..";
{calls}
echo "done";
"#);
        let out = compile_and_run_with_gc_stats(&source);
        assert!(out.success, "{}", out.stderr);
        assert_eq!(out.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&out.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "native mbstring results retained runtime allocations");
}

/// Verifies eval string boxing releases its intermediate native strings and argument casts.
#[test]
fn test_mbstring_eval_string_result_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "mb_strtoupper($subject); mb_strtolower($subject); mb_convert_case($subject, $mode); mb_ucfirst($subject); mb_lcfirst($subject); mb_strimwidth($subject, $start, $width, $marker);\n".repeat(count);
        let source = format!(r#"<?php
$source = $argc > 0 ? '
$subject = "Straße 東京";
$mode = MB_CASE_FOLD;
$start = 0;
$width = 7;
$marker = "..";
{calls}
echo "done";
' : '';
eval($source);
"#);
        let out = compile_and_run_with_gc_stats(&source);
        assert!(out.success, "{}", out.stderr);
        assert_eq!(out.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&out.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "eval mbstring results retained runtime allocations");
}

/// Verifies mixed scalar results and nullable arguments remain balanced in AOT and eval.
#[test]
fn test_mbstring_scalar_result_ownership() {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 24] {
            let calls = r#"
mb_substr($subject, $start, $length); mb_strcut($subject, $start, $length);
mb_scrub($subject); mb_trim($subject); mb_ltrim($subject); mb_rtrim($subject);
mb_str_pad($subject, $width); mb_convert_kana($subject);
mb_substr_count($subject, $needle); mb_ord($subject); mb_chr($codepoint);
mb_strpos($subject, $needle); mb_stripos($subject, $missing);
mb_strrpos($subject, $needle); mb_strripos($subject, $missing);
mb_strstr($subject, $needle, $before); mb_stristr($subject, $missing);
mb_strrchr($subject, $needle); mb_strrichr($subject, $missing);
mb_internal_encoding(); mb_internal_encoding($encoding); mb_language(); mb_http_output();
"#.repeat(count);
            let body = format!(r#"
$subject = "Straße 東京"; $needle = "東"; $missing = "absent";
$start = 0; $length = null; $before = true; $width = 15;
$codepoint = 29483; $encoding = "UTF-8";
{calls}
echo "done";
"#);
            let source = if eval { format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);") }
                else { format!("<?php {body}") };
            let out = compile_and_run_with_gc_stats(&source);
            assert!(out.success, "{}", out.stderr);
            assert_eq!(out.stdout, "done");
            let (allocated, freed) = parse_gc_stats(&out.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "mbstring scalar results leaked with eval={eval}");
    }
}

/// Verifies discarded and overwritten array results release each binary string and boxed owner.
#[test]
fn test_mbstring_array_result_ownership() {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 24] {
            let calls = "mb_str_split($subject); mb_str_split($empty); mb_encoding_aliases($encoding); mb_preferred_mime_name($encoding); $parts = mb_str_split($subject); $aliases = mb_encoding_aliases($encoding); $copy = $parts; $copy[$index] = $replacement;\n".repeat(count);
            let body = format!(r#"
$subject = "Aé猫B"; $empty = ""; $encoding = "ASCII"; $replacement = "changed"; $index = 0;
{calls}
echo "done";
"#);
            let source = if eval { format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);") }
                else { format!("<?php {body}") };
            let out = compile_and_run_with_gc_stats(&source);
            assert!(out.success, "{}", out.stderr);
            assert_eq!(out.stdout, "done");
            let (allocated, freed) = parse_gc_stats(&out.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "mbstring array results leaked with eval={eval}");
    }
}

/// Verifies copied array arguments are released after successful checks and catchable failures.
#[test]
fn test_mbstring_check_array_argument_ownership() {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 24] {
            let calls = "mb_check_encoding($subject); mb_check_encoding($empty); try { mb_check_encoding($subject, $bad); } catch (ValueError) {}\n".repeat(count);
            let body = format!("$subject = [\"names\" => [\"Aé猫B\", \"text\"]]; $empty = []; $bad = \"invalid\";\n{calls}\necho \"done\";");
            let source = if eval { format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);") }
                else { format!("<?php {body}") };
            let out = compile_and_run_with_gc_stats(&source);
            assert!(out.success, "{}", out.stderr);
            assert_eq!(out.stdout, "done");
            let (allocated, freed) = parse_gc_stats(&out.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "mbstring array arguments leaked with eval={eval}");
    }
}

/// Releases mixed setting results and exception messages without retaining scalar argument casts.
#[test]
fn test_mbstring_substitution_result_ownership() {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 24] {
            let calls = "mb_substitute_character($code); mb_substitute_character(); mb_substitute_character($mode); mb_substitute_character(); try { mb_substitute_character($bad); } catch (ValueError) {}\n".repeat(count);
            let body = format!("$code = 33; $mode = \"none\"; $bad = -1;\n{calls}\necho \"done\";");
            let source = if eval { format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);") }
                else { format!("<?php {body}") };
            let out = compile_and_run_with_gc_stats(&source);
            assert!(out.success, "{}", out.stderr);
            assert_eq!(out.stdout, "done");
            let (allocated, freed) = parse_gc_stats(&out.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "mbstring substitution results leaked with eval={eval}");
    }
}
