//! Purpose:
//! Exercises native/eval mbstring calls through shared argument preparation and protected callbacks.
//!
//! Called from:
//! - The focused codegen test binary's string suite.
//!
//! Key details:
//! - Opaque source forces real Magician execution and the generated runtime C callback table.
//! - Native and eval Stringable methods, PHP errors, and coercion diagnostics use real machine code.
//! - Repeated calls isolate capture and bridge ownership from unrelated eval temporaries.

use crate::support::*;

/// Wraps opaque eval source beside a native Stringable class visible through the active eval context.
fn program(body: &str) -> String {
    let body = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!(r#"<?php
class NativeMbText {{
    public function __toString(): string {{ echo "native-text\n"; return "native"; }}
}}
$source = $argc > 0 ? '{body}' : '';
eval($source);
"#)
}

/// Verifies real weak conversions, binary metadata, native/eval Stringable calls, and catchable throws.
#[test]
fn test_mbstring_eval_shared_parameter_coercion() {
    let source = program(r#"
var_dump(mb_strlen(123), mb_strlen(true), mb_strlen(1.5));
var_dump(mb_substr("abcdef", "1.5", "2.9", "8bit"));
try { mb_strlen([]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
class EvalMbText {
    public function __toString(): string { echo "eval-text\n"; mb_internal_encoding("8bit"); return chr(255) . chr(255); }
}
class EvalMbFailure {
    public function __toString(): string { echo "eval-throw\n"; throw new RuntimeException("string callback stopped"); }
}
class EvalMbPlain {}
var_dump(mb_strlen(new EvalMbText()));
var_dump(mb_strlen(new NativeMbText()));
try { mb_strlen(new EvalMbPlain()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { mb_strlen(new EvalMbFailure()); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
echo mb_internal_encoding(), "\n";
"#);
    let output = compile_and_run_capture(&source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "int(3)\nint(1)\nint(3)\nstring(2) \"bc\"\nmb_strlen(): Argument #1 ($string) must be of type string, array given\neval-text\nint(2)\nnative-text\nint(6)\nmb_strlen(): Argument #1 ($string) must be of type string, EvalMbPlain given\neval-throw\nstring callback stopped\n8bit\n");
    assert_eq!(output.stderr, "Deprecated: Implicit conversion from float-string \"1.5\" to int loses precision\nDeprecated: Implicit conversion from float-string \"2.9\" to int loses precision\n");
}

/// Stops later Stringable calls on parameter errors and delays encoding validation until outer coercions finish.
#[test]
fn test_mbstring_eval_shared_coercion_failure_order() {
    let source = program(r#"
class FirstMbText {
    public function __toString(): string { echo "first\n"; return "abcdef"; }
}
class LastMbText {
    public function __toString(): string { echo "last\n"; return "bad-encoding"; }
}
try { mb_strlen([], new LastMbText()); } catch (TypeError $e) { echo "type-first\n"; }
try { mb_substr(new FirstMbText(), "not-a-number", 1, new LastMbText()); }
catch (TypeError $e) { echo "type-second\n"; }
try { mb_substr(new FirstMbText(), 0.5, 1, new LastMbText()); }
catch (ValueError $e) { echo "encoding-last\n"; }
echo mb_strlen("still usable"), "\n";
"#);
    let output = compile_and_run_capture(&source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "type-first\nfirst\ntype-second\nfirst\nlast\nencoding-last\n12\n");
    assert_eq!(output.stderr, "Deprecated: Implicit conversion from float 0.5 to int loses precision\n");
}

/// Verifies repeated successful and throwing Stringable calls balance metadata, argument, and result ownership.
#[test]
fn test_mbstring_eval_shared_coercion_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "mb_strlen($text, $encoding); try { mb_strlen($plain); } catch (TypeError) {} try { mb_strlen($failure); } catch (RuntimeException) {}\n".repeat(count);
        let source = program(&format!(r#"
class RetainedMbText {{ public function __toString(): string {{ return "Straße"; }} }}
class RetainedMbFailure {{ public function __toString(): string {{ throw new RuntimeException("failed"); }} }}
class RetainedMbPlain {{}}
$text = new RetainedMbText(); $failure = new RetainedMbFailure(); $plain = new RetainedMbPlain();
$encoding = "UTF-8";
{calls}
echo "done";
"#));
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "shared eval mbstring invocation retained native owners");
}

/// Evaluates supplied expressions before shared arity validation and exposes catchable ArgumentCountError.
#[test]
fn test_mbstring_eval_shared_arity_errors() {
    let source = program(r#"
function mb_argument_effect(string $value): string { echo $value, "\n"; return $value; }
try { mb_strlen(); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { mb_strlen(mb_argument_effect("a"), mb_argument_effect("b"), mb_argument_effect("c")); }
catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { mb_internal_encoding("UTF-8", "extra"); }
catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { call_user_func("mb_strlen"); }
catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
echo mb_strlen("usable"), "\n";
"#);
    let output = compile_and_run_capture(&source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "mb_strlen() expects at least 1 argument, 0 given\na\nb\nc\nmb_strlen() expects at most 2 arguments, 3 given\nmb_internal_encoding() expects at most 1 argument, 2 given\nmb_strlen() expects at least 1 argument, 0 given\n6\n");
    assert!(output.stderr.is_empty(), "{}", output.stderr);
}

/// Preserves concrete AOT values for PHP weak coercion, ordered failures, and Stringable state changes.
#[test]
fn test_mbstring_aot_shared_parameter_coercion() {
    let source = r#"<?php
class AotMbText {
    public function __toString(): string { echo "stringable\n"; mb_internal_encoding("8bit"); return chr(255) . chr(255); }
}
class AotMbFailure {
    public function __toString(): string { throw new RuntimeException("callback failed"); }
}
class AotMbPlain {}
function mixed_length(mixed $value): int { return mb_strlen($value); }
var_dump(mb_strlen(123), mb_strlen(true), mb_strlen(1.5));
var_dump(mb_substr("abcdef", "1.5", "2.9", "8bit"));
var_dump(mb_strlen(new AotMbText()));
try { mixed_length([]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { mb_strlen(new AotMbPlain()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { mb_strlen(new AotMbFailure()); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
try { mb_strlen("x", 123); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
var_dump(mb_check_encoding(true));
try { mb_substr("a", 0, "bad"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#;
    let output = compile_and_run_capture(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "int(3)\nint(1)\nint(3)\nstring(2) \"bc\"\nstringable\nint(2)\nmb_strlen(): Argument #1 ($string) must be of type string, array given\nmb_strlen(): Argument #1 ($string) must be of type string, AotMbPlain given\ncallback failed\nmb_strlen(): Argument #2 ($encoding) must be a valid encoding, \"123\" given\nbool(true)\nmb_substr(): Argument #3 ($length) must be of type ?int, string given\n");
    assert_eq!(output.stderr, "Deprecated: Implicit conversion from float-string \"1.5\" to int loses precision\nDeprecated: Implicit conversion from float-string \"2.9\" to int loses precision\n");
}

/// Applies strict parameter typing to dynamic original values without executing rejected Stringable methods.
#[test]
fn test_mbstring_aot_shared_strict_parameter_coercion() {
    let source = r#"<?php
declare(strict_types=1);
class StrictMbText { public function __toString(): string { echo "unexpected\n"; return "abc"; } }
function strict_length(mixed $value): int { return mb_strlen($value); }
function strict_named_length(mixed $value): int { return mb_strlen(encoding: "UTF-8", string: $value); }
function strict_spread_length(mixed $value): int { return mb_strlen(...["string" => $value]); }
try { strict_length(123); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { strict_named_length([]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { strict_spread_length(new StrictMbText()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
var_dump(strict_length("猫"));
"#;
    assert_eq!(compile_and_run(source), "mb_strlen(): Argument #1 ($string) must be of type string, int given\nmb_strlen(): Argument #1 ($string) must be of type string, array given\nmb_strlen(): Argument #1 ($string) must be of type string, StrictMbText given\nint(1)\n");
}

/// Evaluates named/spread expressions in source order before coercing them in parameter order.
#[test]
fn test_mbstring_aot_shared_named_parameter_order() {
    let source = r#"<?php
class OrderedMbText {
    public function __toString(): string { echo "coerce-text\n"; return "abcdef"; }
}
class OrderedMbEncoding {
    public function __toString(): string { echo "coerce-encoding\n"; return "8bit"; }
}
function ordered_mb_value(string $label, mixed $value): mixed { echo $label, "\n"; return $value; }
var_dump(mb_substr(encoding: ordered_mb_value("encoding", new OrderedMbEncoding()),
    string: ordered_mb_value("text", new OrderedMbText()),
    start: ordered_mb_value("start", "1"), length: ordered_mb_value("length", "2")));
try { mb_strlen(...["encoding" => ordered_mb_value("encoding", new OrderedMbEncoding()),
    "string" => ordered_mb_value("text", [])]); }
catch (TypeError $e) { echo "type-first\n"; }
"#;
    assert_eq!(compile_and_run(source), "encoding\ntext\nstart\nlength\ncoerce-text\ncoerce-encoding\nstring(2) \"bc\"\nencoding\ntext\ntype-first\n");
}

/// Captures earlier scalar, boxed, and array arguments before later reference assignments can replace them.
#[test]
fn test_mbstring_aot_shared_argument_capture() {
    let source = r#"<?php
function mb_replace_text(string &$text): string { $text = "changed"; return "UTF-8"; }
function mb_replace_mixed(mixed &$text): string { $text = []; return "UTF-8"; }
function mb_change_index(array &$items): string { $items[0] = chr(255); return "UTF-8"; }
$text = str_repeat("a", 64);
var_dump(mb_strlen($text, mb_replace_text($text)), strlen($text));
$mixed = $argc > 0 ? str_repeat("b", 64) : [];
var_dump(mb_strlen($mixed, mb_replace_mixed($mixed)), $mixed);
$items = ["abc"];
var_dump(mb_check_encoding($items, mb_change_index($items)), mb_check_encoding($items));
$text = str_repeat("c", 64);
var_dump(mb_strlen(string: $text, encoding: mb_replace_text($text)), strlen($text));
"#;
    assert_eq!(compile_and_run(source), "int(64)\nint(7)\nint(64)\narray(0) {\n}\nbool(true)\nbool(false)\nint(64)\nint(7)\n");
}

/// Releases captured AOT cells and pinned payloads after successful and throwing shared invocations.
#[test]
fn test_mbstring_aot_shared_argument_capture_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "mb_strlen($subject); try { mb_strlen($subject, $bad); } catch (ValueError) {} try { mb_strlen($invalid); } catch (TypeError) {}\n".repeat(count);
        let source = format!(r#"<?php
$subject = str_repeat("x", 64); $bad = "bad-encoding"; $invalid = $argc > 0 ? [] : "valid";
{calls}
echo "done";
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "captured AOT arguments retained runtime ownership");
}
