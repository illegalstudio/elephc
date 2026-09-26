//! Purpose:
//! Checks shared mbstring detection-order settings and lazy encoding-list coercion.
//!
//! Called from:
//! - The focused codegen string suite through native and opaque eval programs.
//!
//! Key details:
//! - PHP callbacks may alter language defaults or later referenced list elements.
//! - Invalid entries stop before later conversions and preserve the prior detection order.
//! - Native and eval fixtures create equivalent references through their supported syntax.

use crate::support::*;

/// Wraps the same PHP body for direct compilation or opaque interpreter execution.
fn program(body: &str, eval: bool, strict: bool) -> String {
    if !eval { return format!("<?php declare(strict_types={}); {body}", usize::from(strict)); }
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Shares getter/setter state, canonical names, named parameters, and callable invocation with eval.
#[test]
fn test_mbstring_detect_order_settings() {
    let body = r#"
namespace DetectionOrder;
function show_order(): void {
    $order = mb_detect_order();
    if (is_array($order)) { echo implode(",", $order), "\n"; }
}
show_order();
echo Mb_DeTeCt_OrDeR(encoding: "sjis-win, UTF-8, ASCII") ? "set\n" : "failed\n";
show_order();
$set = mb_detect_order(...);
echo $set(["UTF-8", "ASCII", "UTF-8"]) ? "set\n" : "failed\n";
$order = call_user_func("mb_detect_order", null);
if (is_array($order)) { echo implode(",", $order), "\n"; }
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval, false)),
            "ASCII,UTF-8\nset\nSJIS-win,UTF-8,ASCII\nset\nUTF-8,ASCII,UTF-8\n");
    }
}

/// Resolves each auto token after its own callback while stopping at the first invalid name.
#[test]
fn test_mbstring_detect_order_callback_sequence() {
    let body = r#"
class DetectionEncoding {
    public function __construct(public string $label, public string $name, public string $language) {}
    public function __toString(): string {
        echo $this->label, "\n";
        if ($this->language !== "") { mb_language($this->language); }
        return $this->name;
    }
}
function show_detection_order(): void {
    $order = mb_detect_order();
    if (is_array($order)) { echo implode(",", $order), "\n"; }
}
mb_language("neutral");
mb_detect_order(["auto", new DetectionEncoding("later", "UTF-8", "Japanese")]);
show_detection_order();
mb_language("neutral");
mb_detect_order([new DetectionEncoding("first", "UTF-8", "Japanese"), "auto"]);
show_detection_order();
try { mb_detect_order([new DetectionEncoding("invalid", "bad", "neutral"), new DetectionEncoding("skipped", "UTF-8", "Japanese")]); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
show_detection_order();
"#;
    for eval in [false, true] {
        for strict in if eval { &[false][..] } else { &[false, true][..] } {
            assert_eq!(compile_and_run(&program(body, eval, *strict)),
                "later\nASCII,UTF-8,UTF-8\nfirst\nUTF-8,ASCII,JIS,UTF-8,EUC-JP,SJIS\ninvalid\nmb_detect_order(): Argument #1 ($encoding) contains invalid encoding \"bad\"\nUTF-8,ASCII,JIS,UTF-8,EUC-JP,SJIS\n");
        }
    }
}

/// Preserves nullable-entry casts and Stringable exceptions without coercing later values.
#[test]
fn test_mbstring_detect_order_entry_failures() {
    let body = r#"
class DetectionFailure {
    public function __toString(): string { echo "throwing\n"; throw new RuntimeException("stopped"); }
}
class DetectionLater {
    public function __toString(): string { echo "later\n"; return "UTF-8"; }
}
mb_detect_order("ASCII");
try { mb_detect_order([null, new DetectionLater()]); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
try { mb_detect_order([new DetectionFailure(), new DetectionLater()]); }
catch (Throwable $error) { echo $error->getMessage(), "\n"; }
$order = mb_detect_order();
if (is_array($order)) { echo implode(",", $order), "\n"; }
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval, true)),
            "mb_detect_order(): Argument #1 ($encoding) contains invalid encoding \"\"\nthrowing\nstopped\nASCII\n");
    }
}

/// Does not retain one copied entry or temporary cast result per repeated list update.
#[test]
fn test_mbstring_detect_order_entry_ownership() {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 24] {
            let calls = "mb_detect_order($list);\n".repeat(count);
            let source = program(&format!(r#"
class DetectionOwned {{ public function __toString(): string {{ return "UTF-8"; }} }}
$list = ["ASCII", new DetectionOwned()];
{calls}
echo "done";
"#), eval, false);
            let output = compile_and_run_with_gc_stats(&source);
            assert!(output.success, "{}", output.stderr);
            assert_eq!(output.stdout, "done");
            let (allocated, freed) = parse_gc_stats(&output.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "encoding-list entry ownership grew; eval={eval}");
    }
}

/// Casts ordinary list entries without scalar-parameter restrictions and preserves resource aliases.
#[test]
fn test_mbstring_detect_order_entry_casts_and_resource() {
    let body = r#"
class DetectionPlain {}
try { mb_detect_order([1.5]); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
try { @mb_detect_order([[]]); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
try { mb_detect_order([new DetectionPlain()]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
$handle = fopen("php://temp", "w+");
try { mb_detect_order([$handle]); }
catch (ValueError $error) { echo str_contains($error->getMessage(), "Resource id #") ? "resource\n" : "wrong\n"; }
fwrite($handle, "alive");
rewind($handle);
echo fread($handle, 5), "\n";
fclose($handle);
"#;
    for eval in [false, true] {
        let source = if eval {
            program(&body.replace("@mb_detect_order", "mb_detect_order"), true, false)
                .replace("eval($source)", "@eval($source)")
        } else { program(body, false, false) };
        assert_eq!(compile_and_run(&source),
            "mb_detect_order(): Argument #1 ($encoding) contains invalid encoding \"1.5\"\nmb_detect_order(): Argument #1 ($encoding) contains invalid encoding \"Array\"\nObject of class DetectionPlain could not be converted to string\nresource\nalive\n");
    }
}

/// Builds an encoding list whose first entry changes the value referenced by its second entry.
fn reference_program(eval: bool) -> String {
    let body = r#"
class DetectionReference {
    public ?Closure $change = null;
    public function __toString(): string {
        $change = $this->change;
        if ($change !== null) { $change(); }
        return "ASCII";
    }
}
$changer = new DetectionReference();
$list = [$changer, "bad"];
$next_encoding =& $list[1];
$changer->change = function () use (&$next_encoding): void { $next_encoding = "UTF-8"; };
mb_detect_order($list);
$order = mb_detect_order();
if (is_array($order)) { echo implode(",", $order), "\n"; }
echo $next_encoding, "\n";
"#;
    let body = if eval {
        body.replace("$list = [$changer, \"bad\"];\n$next_encoding =& $list[1];",
            "$next_encoding = \"bad\";\n$list = [$changer, &$next_encoding];")
    } else { body.to_string() };
    program(&body, eval, false)
}

/// Reads a later native reference after the previous Stringable callback mutates it.
#[test]
fn test_mbstring_detect_order_later_reference() {
    assert_eq!(compile_and_run(&reference_program(false)), "ASCII,UTF-8\nUTF-8\n");
}

/// Preserves eval array-reference metadata when native invocation clones the boxed array argument.
#[test]
fn test_mbstring_detect_order_eval_later_reference() {
    assert_eq!(compile_and_run(&reference_program(true)), "ASCII,UTF-8\nUTF-8\n");
}

/// Resolves eval references to array slots, nested arrays, properties, aliases, and static storage lazily.
#[test]
fn test_mbstring_detect_order_eval_reference_targets() {
    let body = r#"
function detection_set_nested(string &$value): void { $value = "SJIS"; }
class DetectionReferenceState {
    public string $name = "bad";
    public static string $shared = "bad";
}
class DetectionReferenceCallback {
    public ?Closure $change = null;
    public function __toString(): string {
        $change = $this->change;
        if ($change !== null) { $change(); }
        return "ASCII";
    }
}
$array = ["bad"];
$nested = [["bad"]];
$holder = new DetectionReferenceState();
$alias = new DetectionReferenceState();
$source = "bad";
$alias->name =& $source;
$static_source = "bad";
DetectionReferenceState::$shared =& $static_source;
$callback = new DetectionReferenceCallback();
$list = ["callback" => $callback, "array" => &$array[0], "nested" => &$nested[0][0],
    "property" => &$holder->name, "alias" => &$alias->name, "static" => &DetectionReferenceState::$shared];
$callback->change = function () use (&$array, &$nested, $holder, &$source): void {
    $array[0] = "UTF-8";
    detection_set_nested($nested[0][0]);
    $holder->name = "ASCII";
    $source = "UTF-16LE";
    DetectionReferenceState::$shared = "UTF-32BE";
};
mb_detect_order($list);
$order = mb_detect_order();
if (is_array($order)) { echo implode(",", $order), "\n"; }
"#;
    assert_eq!(compile_and_run(&program(body, true, false)), "ASCII,UTF-8,SJIS,ASCII,UTF-16LE,UTF-32BE\n");
}

/// Releases copied eval reference values and intermediate array/property owners after every call.
#[test]
fn test_mbstring_detect_order_eval_reference_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "mb_detect_order($list);\n".repeat(count);
        let source = program(&format!(r#"
class DetectionReferenceOwned {{
    public string $name = "UTF-8";
    public static string $shared = "UTF-8";
}}
$value = "UTF-8";
$array = ["UTF-8"];
$nested = [["UTF-8"]];
$holder = new DetectionReferenceOwned();
$alias = new DetectionReferenceOwned();
$alias->name =& $value;
DetectionReferenceOwned::$shared =& $value;
$list = [&$value, &$array[0], &$nested[0][0], &$holder->name, &$alias->name, &DetectionReferenceOwned::$shared];
{calls}
echo "done";
"#), true, false);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "eval reference read ownership grew with repeated calls");
}

/// Keeps ordinary list values in the retained COW snapshot when a callback mutates caller storage.
#[test]
fn test_mbstring_detect_order_callback_list_cow() {
    let body = r#"
class DetectionListCow {
    public ?Closure $change = null;
    public function __toString(): string {
        $change = $this->change;
        if ($change !== null) { $change(); }
        return "ASCII";
    }
}
$callback = new DetectionListCow();
$source_list = [$callback, "UTF-8"];
$callback->change = function () use (&$source_list): void { $source_list[1] = "bad"; };
mb_detect_order($source_list);
$order = mb_detect_order();
if (is_array($order)) { echo implode(",", $order), "\n"; }
echo $source_list[1], "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval, false)), "ASCII,UTF-8\nbad\n");
    }
}

/// Keeps a referenced eval resource alive after its copied entry fails encoding-name validation.
#[test]
fn test_mbstring_detect_order_eval_reference_resource() {
    let body = r#"
$handle = fopen("php://temp", "w+");
$list = [&$handle];
try { mb_detect_order($list); }
catch (ValueError $error) { echo str_contains($error->getMessage(), "Resource id #") ? "resource:" : "wrong:"; }
fwrite($handle, "alive");
rewind($handle);
echo fread($handle, 5);
fclose($handle);
"#;
    assert_eq!(compile_and_run(&program(body, true, false)), "resource:alive");
}

/// Resolves eval list references through native instance and static property getters after callbacks.
#[test]
fn test_mbstring_detect_order_eval_native_reference_targets() {
    let body = r#"
class DetectionNativeCallback {
    public function __construct(public DetectionNativeReferences $holder) {}
    public function __toString(): string {
        $this->holder->encoding = "UTF-8";
        DetectionNativeReferences::$shared = "ASCII";
        return "SJIS";
    }
}
$holder = new DetectionNativeReferences();
$list = [new DetectionNativeCallback($holder), &$holder->encoding, &DetectionNativeReferences::$shared];
mb_detect_order($list);
$order = mb_detect_order();
if (is_array($order)) { echo implode(",", $order), "\n"; }
"#;
    let source = program(body, true, false).replacen("<?php", r#"<?php
class DetectionNativeReferences {
    public string $encoding = "bad";
    public static string $shared = "bad";
}
"#, 1);
    assert_eq!(compile_and_run(&source), "SJIS,UTF-8,ASCII\n");
}

/// Ignores list keys, rejects empty inputs atomically, and returns independently mutable getter arrays.
#[test]
fn test_mbstring_detect_order_validation_and_copies() {
    let body = r#"
mb_detect_order(["primary" => "utf8", "fallback" => "ASCII"]);
$copy = mb_detect_order();
if (is_array($copy)) { $copy[0] = "SJIS"; }
try { mb_detect_order([]); } catch (ValueError $error) { echo "empty array\n"; }
try { mb_detect_order(""); } catch (ValueError $error) { echo "empty string\n"; }
try { mb_detect_order(["ASCII", false]); } catch (ValueError $error) { echo "bad entry\n"; }
$current = mb_detect_order();
if (is_array($current)) { echo implode(",", $current), "\n"; }
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval, false)),
            "empty array\nempty string\nbad entry\nUTF-8,ASCII\n");
    }
}
