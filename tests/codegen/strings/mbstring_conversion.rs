//! Purpose:
//! Checks public string and recursive-array conversion through native and opaque eval backends.
//!
//! Called from:
//! - The focused codegen string integration suite.
//!
//! Key details:
//! - Expected bytes come from PHP, with binary conversions compared as hexadecimal or round trips.
//! - Arrays preserve key identity, nesting, collisions, scalar values, and independent COW ownership.

use crate::support::*;

/// Wraps one PHP body for direct compilation or opaque evaluation with equivalent source bytes.
fn program(body: &str, eval: bool) -> String {
    if !eval { return format!("<?php {body}"); }
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Matches PHP conversion string calls in both execution backends.
#[test]
fn test_mbstring_conversion_string_calls() {
    let body = r#"
namespace Conversion;
echo bin2hex(Mb_CoNvErT_EnCoDiNg(string: "café", to_encoding: "UTF-16LE", from_encoding: "UTF-8")), "\n";
$convert = mb_convert_encoding(...);
echo $convert("caf" . chr(233), "UTF-8", "ISO-8859-1"), "\n";
mb_internal_encoding("ISO-8859-1");
echo call_user_func("mb_convert_encoding", "caf" . chr(233), "UTF-8"), "\n";
echo mb_convert_encoding("café", "UTF-8", ["UTF-8", "ISO-8859-1"]), "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "630061006600e900\ncafé\ncafé\ncafé\n", "eval={eval}");
    }
}

/// Matches PHP conversion arrays in both execution backends.
#[test]
fn test_mbstring_conversion_arrays() {
    let body = r#"$source = ["clé" => ["café", 12, 1.5, true, null], "keep" => -0.0, 7 => false];
$converted = mb_convert_encoding($source, "ISO-8859-1", "UTF-8");
$roundtrip = mb_convert_encoding($converted, "UTF-8", "ISO-8859-1");
echo json_encode($source), "\n", json_encode($roundtrip), "\n";
$keys = ["1" . chr(0) => "a" . chr(0), 1 => "b" . chr(0)];
$result = mb_convert_encoding($keys, "UTF-8", "UTF-16LE");
echo json_encode($result), "\n";
$collisions = ["é" => "first", "è" => "second", "" => []];
echo json_encode(mb_convert_encoding($collisions, "ASCII", "UTF-8")), "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "{\"cl\\u00e9\":[\"caf\\u00e9\",12,1.5,true,null],\"keep\":-0,\"7\":false}\n{\"cl\\u00e9\":[\"caf\\u00e9\",12,1.5,true,null],\"keep\":-0,\"7\":false}\n{\"1\":\"a\",\"1\":\"b\"}\n{\"?\":\"first\",\"\":[]}\n", "eval={eval}");
    }
}

/// Matches PHP conversion callbacks in both execution backends.
#[test]
fn test_mbstring_conversion_callbacks() {
    let body = r#"
class ConversionText {
    public function __construct(public string $label, public string $text) {}
    public function __toString(): string {
        echo $this->label, "\n";
        if ($this->label === "source") { mb_substitute_character("long"); }
        return $this->text;
    }
}
echo mb_convert_encoding(new ConversionText("input", "猫"), new ConversionText("to", "ASCII"), [new ConversionText("source", "UTF-8")]), "\n";
try { mb_convert_encoding("", "bad", [new ConversionText("skipped", "UTF-8")]); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
try { mb_convert_encoding("", "UTF-8", [new ConversionText("invalid", "bad"), new ConversionText("skipped", "UTF-8")]); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
try { mb_convert_encoding("", "UTF-8", []); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "input\nto\nsource\nU+732B\nmb_convert_encoding(): Argument #2 ($to_encoding) must be a valid encoding, \"bad\" given\ninvalid\nmb_convert_encoding(): Argument #3 ($from_encoding) contains invalid encoding \"bad\"\nmb_convert_encoding(): Argument #3 ($from_encoding) must specify at least one encoding\n", "eval={eval}");
    }
}

/// Reads current eval root and nested references after source-list callbacks replace caller storage.
#[test]
fn test_mbstring_conversion_input_references() {
    let body = r#"
class ConversionReference {
    public ?Closure $change = null;
    public function __toString(): string {
        $change = $this->change;
        if ($change !== null) { $change(); }
        return "UTF-8";
    }
}
function convert_references(): void {
    $slot = "before";
    $nested = ["value" => &$slot];
    $input = ["nested" => $nested, "direct" => &$slot];
    $changer = new ConversionReference();
    $changer->change = function () use (&$slot): void { $slot = "after"; };
    echo json_encode(mb_convert_encoding($input, "UTF-8", [$changer])), "\n";
    $input = [&$slot];
    $changer->change = function () use (&$input, &$slot): void {
        $input = ["replacement"];
        $slot = "replaced";
    };
    echo json_encode(mb_convert_encoding($input, "UTF-8", [$changer])), ":", json_encode($input), "\n";
    $other = "old";
    $child = [&$slot];
    $input = [&$child];
    $changer->change = function () use (&$child, &$other): void {
        $child = [&$other];
        $other = "child";
    };
    echo json_encode(mb_convert_encoding($input, "UTF-8", [$changer])), "\n";
}
convert_references();
"#;
    assert_eq!(compile_and_run(&program(body, true)), "{\"nested\":{\"value\":\"after\"},\"direct\":\"after\"}\n[\"replaced\"]:[\"replacement\"]\n[[\"child\"]]\n");
}

/// Preserves a native array element alias through conversion source callbacks.
#[test]
fn test_mbstring_conversion_native_input_reference() {
    let source = r#"<?php
class ConversionReference {
    public ?Closure $change = null;
    public function __toString(): string {
        $change = $this->change;
        if ($change !== null) { $change(); }
        return "UTF-8";
    }
}
$input = ["before"];
$slot =& $input[0];
$changer = new ConversionReference();
$changer->change = function () use (&$slot): void { $slot = "after"; };
echo json_encode(mb_convert_encoding($input, "UTF-8", [$changer]));
"#;
    assert_eq!(compile_and_run(source), "[\"after\"]");
}

/// Keeps references created from closure captures valid after that activation returns, without mbstring.
#[test]
fn test_mbstring_conversion_eval_captured_reference_lifetime() {
    let body = r#"
function captured_reference(): void {
    $value = "before";
    $array = [];
    $build = function () use (&$array, &$value): void { $array = [&$value]; };
    $build();
    $value = "after";
    echo $array[0];
}
captured_reference();
"#;
    assert_eq!(compile_and_run(&program(body, true)), "after");
}

/// Preserves reference metadata while a copied eval array detaches its ordinary payload for COW.
#[test]
fn test_mbstring_conversion_eval_array_reference_copy() {
    let body = r#"
$slot = "before";
$input = [&$slot, "fixed"];
$copy = $input;
$copy[1] = "changed";
$slot = "after";
echo json_encode(mb_convert_encoding($copy, "UTF-8", "UTF-8")), "\n";
echo json_encode(mb_convert_encoding($input, "UTF-8", "UTF-8")), "\n";
echo $copy[0], ":", $input[0];
"#;
    assert_eq!(compile_and_run(&program(body, true)), "[\"after\",\"changed\"]\n[\"after\",\"fixed\"]\nafter:after");
}

/// Writes through references preserved by both indexed and associative array copies.
#[test]
fn test_mbstring_conversion_eval_reference_write() {
    let body = r#"
$slot = "before";
$input = [&$slot];
$copy = $input;
$copy[0] = "changed";
echo $slot, ":", json_encode(mb_convert_encoding($input, "UTF-8", "UTF-8")), ":", json_encode(mb_convert_encoding($copy, "UTF-8", "UTF-8")), "\n";
$assoc = ["key" => &$slot];
$other = $assoc;
$other["key"] = "again";
echo $slot, ":", json_encode(mb_convert_encoding($assoc, "UTF-8", "UTF-8"));
"#;
    assert_eq!(compile_and_run(&program(body, true)), "changed:[\"changed\"]:[\"changed\"]\nagain:{\"key\":\"again\"}");
}

/// Keeps local references alive when their creating function returns its array.
#[test]
fn test_mbstring_conversion_eval_returned_reference() {
    let body = r#"
function make_reference(): array { $value = "alive"; return [&$value]; }
$input = make_reference();
echo json_encode(mb_convert_encoding($input, "UTF-8", "UTF-8")), "\n";
$copy = $input;
$copy[0] = "updated";
echo json_encode(mb_convert_encoding($input, "UTF-8", "UTF-8"));
function make_assoc_reference(): array { $value = "assoc"; return ["key" => &$value]; }
$assoc = make_assoc_reference();
$assoc_copy = $assoc;
$assoc_copy["key"] = "changed";
echo "\n", json_encode(mb_convert_encoding($assoc, "UTF-8", "UTF-8")), ":", json_encode($assoc_copy);
"#;
    assert_eq!(compile_and_run(&program(body, true)), "[\"alive\"]\n[\"alive\"]\n{\"key\":\"assoc\"}:{\"key\":\"changed\"}");
}

/// Replacing an array must not attach its former references to reused runtime addresses.
#[test]
fn test_mbstring_conversion_eval_reference_address_reuse() {
    let body = r#"
$slot = "before";
$input = [&$slot];
$input = ["first"];
$input = ["second"];
$slot = "after";
echo json_encode(mb_convert_encoding($input, "UTF-8", "UTF-8"));
"#;
    assert_eq!(compile_and_run(&program(body, true)), "[\"second\"]");
}

/// Ordinary assignment detaches a referenced value while nested array writes remain shared.
#[test]
fn test_mbstring_conversion_eval_reference_value_copy() {
    let body = r#"
$value = "before";
$input = [&$value];
$copy = $value;
$value = "after";
echo $copy, ":", $input[0], "\n";
$nested = ["initial"];
$outer = [&$nested];
$nested[0] = "nested";
echo json_encode(mb_convert_encoding($outer, "UTF-8", "UTF-8"));
"#;
    assert_eq!(compile_and_run(&program(body, true)), "before:after\n[[\"nested\"]]");
}

/// A referenced scalar remains independent across ordinary parameters, captures, and returns.
#[test]
fn test_mbstring_conversion_eval_reference_value_boundaries() {
    let body = r#"
function replace_value($value): void { $value = "parameter"; }
function read_value($value) { return $value; }
$value = "before";
$input = [&$value];
$capture = function () use ($value) { return $value; };
replace_value($value);
$returned = read_value($value);
$plain = [$value];
$value = "after";
echo $returned, ":", $capture(), ":", json_encode($plain), ":", json_encode(mb_convert_encoding($input, "UTF-8", "UTF-8"));
"#;
    assert_eq!(compile_and_run(&program(body, true)), "before:before:[\"before\"]:[\"after\"]");
}

/// Keeps a returned closure and array connected through a reference after their creator exits.
#[test]
fn test_mbstring_conversion_eval_returned_reference_closure() {
    let body = r#"
function make_pair(): array {
    $value = "before";
    $change = function () use (&$value) { $value = "after"; };
    return [[&$value], $change];
}
$pair = make_pair();
$input = $pair[0];
$change = $pair[1];
$change();
echo json_encode(mb_convert_encoding($input, "UTF-8", "UTF-8"));
"#;
    assert_eq!(compile_and_run(&program(body, true)), "[\"after\"]");
}

/// Reading an element must not preserve an orphan reference across a later array copy.
#[test]
fn test_mbstring_conversion_eval_reference_read_then_copy() {
    let body = r#"
$value = "before";
$input = [&$value];
echo $input[0], ":";
unset($value);
$copy = $input;
$copy[0] = "after";
echo json_encode(mb_convert_encoding($input, "UTF-8", "UTF-8")), ":", json_encode($copy);
"#;
    assert_eq!(compile_and_run(&program(body, true)), "before:[\"before\"]:[\"after\"]");
}

/// Reads and mutates an object reference while an ordinary copy keeps the previous object identity.
#[test]
fn test_mbstring_conversion_eval_reference_object_copy() {
    let body = r#"
$value = new stdClass();
$value->text = "before";
$input = [&$value];
$copy = $value;
$value = new stdClass();
$value->text = "after";
echo $copy->text, ":", $value->text;
"#;
    assert_eq!(compile_and_run(&program(body, true)), "before:after");
}

/// Matches PHP conversion array cow in both execution backends.
#[test]
fn test_mbstring_conversion_array_cow() {
    let body = r#"
$source = ["plain" => "café", "nested" => ["猫"]];
$result = mb_convert_encoding($source, "UTF-8", "UTF-8");
$copy = $result;
$copy["plain"] = "changed";
echo json_encode($source), "\n", json_encode($result), "\n", json_encode($copy), "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "{\"plain\":\"caf\\u00e9\",\"nested\":[\"\\u732b\"]}\n{\"plain\":\"caf\\u00e9\",\"nested\":[\"\\u732b\"]}\n{\"plain\":\"changed\",\"nested\":[\"\\u732b\"]}\n", "eval={eval}");
    }
}

/// Balances repeated nested-array materialization, retained copies, and result destruction.
#[test]
fn test_mbstring_conversion_array_ownership() {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 24] {
            let calls = "$result = mb_convert_encoding($input, \"UTF-8\", \"UTF-8\");\n".repeat(count);
            let body = format!(r#"$input = ["label" => ["café", "猫"], "amount" => 12]; {calls} echo json_encode($result);"#);
            let output = compile_and_run_with_gc_stats(&program(&body, eval));
            assert!(output.success, "{}", output.stderr);
            assert_eq!(output.stdout, r#"{"label":["caf\u00e9","\u732b"],"amount":12}"#);
            let (allocated, freed) = parse_gc_stats(&output.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "conversion array ownership grew; eval={eval}");
    }
}

/// Preserves ordinary Mixed value copies while explicit reference aliases remain connected.
#[test]
fn test_mbstring_conversion_mixed_copy_regression() {
    let source = r#"<?php
function boxed_array(): mixed { return ["item" => "before"]; }
$original = boxed_array();
$copy = $original;
$copy["item"] = "copy";
$other = boxed_array();
$selected = $argc > 0 ? $original : $other;
$selected["item"] = "selected";
$alias =& $original;
$alias["item"] = "alias";
echo json_encode($original), ":", json_encode($copy), ":", json_encode($selected);
"#;
    assert_eq!(compile_and_run(source), r#"{"item":"alias"}:{"item":"copy"}:{"item":"selected"}"#);
}

/// Detaches boxed array values assigned inside another expression before either copy mutates.
#[test]
fn test_mbstring_conversion_mixed_assignment_expression() {
    let source = r#"<?php
function boxed_array(): mixed { return ["item" => "before"]; }
$original = boxed_array();
echo json_encode($copy = $original), ":";
$copy["item"] = "copy";
$outer = ($inner = $original);
$inner["item"] = "inner";
echo json_encode($original), ":", json_encode($copy), ":", json_encode($inner), ":", json_encode($outer);
"#;
    assert_eq!(compile_and_run(source), r#"{"item":"before"}:{"item":"before"}:{"item":"copy"}:{"item":"inner"}:{"item":"before"}"#);
}

/// Preserves signed zero through eval negation and JSON serialization independently of conversion.
#[test]
fn test_mbstring_conversion_eval_signed_zero_regression() {
    let body = r#"$zero = 0.0; echo json_encode(-$zero), ":", json_encode(-(-$zero));"#;
    assert_eq!(compile_and_run(&program(body, true)), "-0:0");
}

/// Iterates converted numeric string and integer keys without relooking up their normalized names.
#[test]
fn test_mbstring_conversion_exact_key_iteration() {
    let body = r#"
$input = ["1" . chr(0) => "a" . chr(0), 1 => "b" . chr(0)];
$result = mb_convert_encoding($input, "UTF-8", "UTF-16LE");
foreach ($result as $key => $value) { echo gettype($key), ":", (string)$key, "=", (string)$value, "\n"; }
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "string:1=a\ninteger:1=b\n", "eval={eval}");
    }
}
