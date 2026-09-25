//! Purpose:
//! Checks live mb_convert_variables mutation through native and opaque eval calls.
//!
//! Called from:
//! - The focused codegen string integration suite.
//!
//! Key details:
//! - PHP 8.5 fixtures pin private properties, nested references, COW, and partial writes.
//! - Binary output is rendered as hexadecimal so invalid source bytes remain observable.

use crate::support::*;

/// Converts nested referenced strings and private object properties without changing array keys.
#[test]
fn test_mbstring_variables_nested_references_and_private_properties() {
    let source = r#"<?php
class VariableBox {
    private string $secret;
    public function __construct(string $value) { $this->secret = $value; }
    public function secret(): string { return $this->secret; }
}
$box = new VariableBox("cr" . chr(232) . "me");
$root = ["cl" . chr(233) => "caf" . chr(233), "box" => $box];
$leaf =& $root["cl" . chr(233)];
$copy = $root;
$source = Mb_CoNvErT_VaRiAbLeS("UTF-8", "ISO-8859-1", $root);
echo $source, "|", bin2hex($leaf), "|", bin2hex($root["cl" . chr(233)]), "|", bin2hex($root["box"]->secret()), "|", bin2hex($copy["cl" . chr(233)]), "\n";
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|636166e9|636166c3a9|6372c3a86d65|636166e9\n");
}

/// Repeated variadic roots write through the same caller reference at each argument position.
#[test]
fn test_mbstring_variables_repeated_roots() {
    let source = r#"<?php
$value = chr(233);
$alias =& $value;
$source = mb_convert_variables("UTF-8", "ISO-8859-1", $alias, $value);
echo $source, "|", bin2hex($value), "\n";
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|c383c2a9\n");
}

/// Opaque eval passes the original scope cell to the same live conversion host.
#[test]
fn test_mbstring_variables_eval_live_root() {
    let source = r#"<?php
$value = chr(233);
$fragment = $argc > 0 ? 'echo mb_convert_variables("UTF-8", "ISO-8859-1", $value), "|", bin2hex($value);' : '';
eval($fragment);
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|c3a9");
}

/// Named inputs retain source order while the variable argument keeps its live slot.
#[test]
fn test_mbstring_variables_named_arguments() {
    let source = r#"<?php
$value = chr(233);
$source = mb_convert_variables(var: $value, to_encoding: "UTF-8", from_encoding: "ISO-8859-1");
echo $source, "|", bin2hex($value);
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|c3a9");
}

/// Detects one source across roots, then mutates nested arrays and dynamic properties.
#[test]
fn test_mbstring_variables_detect_multiple_roots() {
    let source = r#"<?php
$items = ["first" => chr(233), "nested" => [chr(232)]];
$box = new stdClass();
$box->text = chr(233);
$from = mb_convert_variables("UTF-8", ["UTF-8", "ISO-8859-1"], $items, $box);
echo $from, "|", bin2hex($items["first"]), "|", bin2hex($items["nested"][0]), "|", bin2hex($box->text);
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|c3a9|c3a8|c3a9");
}

/// A by-reference function parameter remains connected to the caller's storage.
#[test]
fn test_mbstring_variables_by_reference_parameter() {
    let source = r#"<?php
function normalize_import(&$value): string|false {
    return mb_convert_variables("UTF-8", "ISO-8859-1", $value);
}
$value = chr(233);
$source = normalize_import($value);
echo $source, "|", bin2hex($value);
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|c3a9");
}

/// A first-class callable retains the variadic reference signature.
#[test]
fn test_mbstring_variables_first_class_callable() {
    let source = r#"<?php
$convert = mb_convert_variables(...);
$value = chr(233);
$source = $convert("UTF-8", "ISO-8859-1", $value);
echo $source, "|", bin2hex($value);
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|c3a9");
}

/// Dynamic descriptor invocation forwards a concrete caller reference into the wrapper.
#[test]
fn test_mbstring_variables_dynamic_callable() {
    let source = r#"<?php
$convert = $argc > 0 ? mb_convert_variables(...) : mb_convert_encoding(...);
$value = chr(233);
$source = $convert("UTF-8", "ISO-8859-1", $value);
echo $source, "|", bin2hex($value);
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|c3a9");
}

/// Array elements and object properties are legal direct by-reference roots.
#[test]
fn test_mbstring_variables_element_and_property_roots() {
    let source = r#"<?php
$items = ["text" => chr(233)];
$box = new stdClass();
$box->text = chr(232);
$source = mb_convert_variables("UTF-8", "ISO-8859-1", $items["text"], $box->text);
echo $source, "|", bin2hex($items["text"]), "|", bin2hex($box->text);
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|c3a9|c3a8");
}

/// Unpacked reference entries update their original variables through the variadic ABI.
#[test]
fn test_mbstring_variables_unpacked_references() {
    let source = r#"<?php
$roots = [chr(233)];
$value =& $roots[0];
$source = mb_convert_variables("UTF-8", "ISO-8859-1", ...$roots);
echo $source, "|", bin2hex($value), "|", bin2hex($roots[0]);
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|c3a9|c3a9");
}

/// Unpacked ordinary entries become writable call arguments after array COW separation.
#[test]
fn test_mbstring_variables_unpacked_values_separate_copy() {
    let source = r#"<?php
$roots = [chr(233)];
$copy = $roots;
$source = mb_convert_variables("UTF-8", "ISO-8859-1", ...$roots);
echo $source, "|", bin2hex($roots[0]), "|", bin2hex($copy[0]);
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|c3a9|e9");
}

/// Array-based callable dispatch preserves a referenced argument slot.
#[test]
fn test_mbstring_variables_call_user_func_array_reference() {
    let source = r#"<?php
$arguments = ["UTF-8", "ISO-8859-1", chr(233)];
$value =& $arguments[2];
$source = call_user_func_array("mb_convert_variables", $arguments);
echo $source, "|", bin2hex($value), "|", bin2hex($arguments[2]);
"#;
    assert_eq!(compile_and_run(source), "ISO-8859-1|c3a9|c3a9");
}

/// Reports recursion after earlier string writes have become visible to PHP.
#[test]
fn test_mbstring_variables_recursive_object_partial_write() {
    let source = r#"<?php
class VariableCycle { public string $value; public mixed $next = null; }
$root = new VariableCycle();
$root->value = chr(233);
$root->next = $root;
set_error_handler(function($level, $message) { echo $message, "\n"; });
$result = mb_convert_variables("UTF-8", "ISO-8859-1", $root);
echo ($result === false ? "false" : $result), "|", bin2hex($root->value), "\n";
"#;
    assert_eq!(compile_and_run(source),
        "mb_convert_variables(): Cannot handle recursive references\nfalse|c3a9\n");
}

/// A promoted array storage shape still detects its own reference cycle.
#[test]
fn test_mbstring_variables_recursive_array_partial_write() {
    let source = r#"<?php
$root = [chr(233), null];
$alias =& $root[1];
$alias = $root;
set_error_handler(function($level, $message) { echo $message, "\n"; });
$result = mb_convert_variables("UTF-8", "ISO-8859-1", $root);
echo ($result === false ? "false" : $result), "|", bin2hex($root[0]), "\n";
"#;
    assert_eq!(compile_and_run(source),
        "mb_convert_variables(): Cannot handle recursive references\nfalse|c3a9\n");
}
