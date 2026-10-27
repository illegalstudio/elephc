//! Purpose:
//! Shares opaque-eval catalog compilation for SimpleXML access and projection regressions.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on all executable targets.
//!
//! Key details:
//! - Each pair shares one compilation and runs in two independent processes.
//! - Each original fixture retains its caller contexts, exact output and clean-heap check.

use crate::support::*;

/// Key leases and warning caller scopes share compilation while keeping separate native heaps.
#[test]
fn test_simplexml_eval_access_owners_bundle() {
    if skip_without_xml_native("test_simplexml_eval_access_owners_bundle") { return; }
    let runs = compile_and_run_with_heap_debug_per_argv(r####"<?php
if ($argc === 1) {
$xml = new SimpleXMLElement('<r z="0"/>');
$source = $argc > 0 ? '
class ThrowingXmlEmptyKey {
    public function __toString(): string { throw new RuntimeException("cast"); }
    public function __destruct() { echo "drop|"; }
}
function freshXmlEmptyKey(): mixed { return new ThrowingXmlEmptyKey(); }
$key = "z";
echo "probes|";
for ($i = 0; $i < 32; $i++) {
    if ($i === 0) { echo "empty-loop|"; }
    if (!empty($xml["missing" . str_repeat("x", 40)]) || !empty($xml[$key])) { echo "wrong|"; }
    if ($i === 0) { echo "isset-loop|"; }
    if (isset($xml["missing" . str_repeat("x", 40)]) || !isset($xml[$key])) { echo "wrong|"; }
    if ($i === 0) { echo "fresh-loop|"; }
    if (isset((new SimpleXMLElement("<r/>"))["missing"]) || !empty((new SimpleXMLElement("<r/>"))["missing"])) { echo "wrong|"; }
    if ($i === 0) { echo "chain-loop|"; }
    if (!empty($xml?->missing["key" . str_repeat("x", 40)])) { echo "wrong|"; }
}
echo "empty|";
try { empty($xml[freshXmlEmptyKey()]); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; }
echo "isset|";
try { isset($xml[freshXmlEmptyKey()]); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; }
echo $key;
' : '';
} else {
$xml = new SimpleXMLElement('<r><item>a</item><item>b</item></r>');
set_error_handler(function(int $level, string $message): bool { echo $message, '|'; return true; });
$source = $argc > 0 ? '
function evaluatedXmlWarning(SimpleXMLElement $xml): void { $xml->item[4] = "f"; }
class EvaluatedXmlWarning {
    public function instanceWrite(SimpleXMLElement $xml): void { $xml->item[4] = "i"; }
    public static function staticWrite(SimpleXMLElement $xml): void { $xml->item[4] = "s"; }
}
$xml->item = "duplicate";
$xml->item[4] = "top";
evaluatedXmlWarning(new SimpleXMLElement("<r><item>a</item></r>"));
(new EvaluatedXmlWarning())->instanceWrite(new SimpleXMLElement("<r><item>a</item></r>"));
EvaluatedXmlWarning::staticWrite(new SimpleXMLElement("<r><item>a</item></r>"));
' : '';
}
eval($source);
if ($argc !== 1) { restore_error_handler(); }
"####, &[&[], &["warning-callers"]]);
    let cases = [
        ("test_simplexml_eval_empty_dimension_key_owners", "probes|empty-loop|isset-loop|fresh-loop|chain-loop|empty|drop|cast|isset|drop|cast|z"),
        ("test_simplexml_eval_assignment_warning_callers", concat!(
            "Unknown: Cannot assign to an array of nodes (duplicate subnodes or attr detected)|",
            "Unknown: Cannot add element item number 4 when only 2 such elements exist|",
            "evaluatedXmlWarning(): Cannot add element item number 4 when only 1 such elements exist|",
            "EvaluatedXmlWarning::instanceWrite(): Cannot add element item number 4 when only 1 such elements exist|",
            "EvaluatedXmlWarning::staticWrite(): Cannot add element item number 4 when only 1 such elements exist|",
        )),
    ];
    assert_eq!(runs.len(), cases.len());
    let mut failures = Vec::new();
    for (out, (name, expected)) in runs.iter().zip(cases) {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("case={name} stdout={:?} expected={expected:?}\n{}", out.stdout, out.stderr));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Shares one native catalog compile across two independent process fixtures.
/// Original eval forms, caller scopes, outputs and ownership assertions are retained.
#[test]
fn test_simplexml_eval_projection_and_nullsafe_bundle() {
    if skip_without_xml_native("test_simplexml_eval_projection_and_nullsafe_bundle") { return; }
    let runs = compile_and_run_with_heap_debug_per_argv(r####"<?php
function emptyChainName(): string { echo 'name|'; return 'item'; }
function emptyChainIndex(): string { echo 'index|'; return 'z'; }
function emptyChains(mixed $node): void {
    echo (int)empty($node?->child->{emptyChainName()}), '|';
    echo (int)empty($node?->child[emptyChainIndex()]), '|';
}
if ($argc === 1) {
$xml = new SimpleXMLElement('<r><i>1</i><i>2</i><i>3</i></r>');
$items = $xml->i; $items->rewind(); $first = $items->current();
echo json_encode((array)$items), '|', (string)$items->current(), ':', (int)($first === $items->current()), '|';
foreach ($items as $item) { echo $item, json_encode($items), ';'; }
echo '|';
$xml = new SimpleXMLElement('<r xmlns:p="urn:p"><a><p:b/></a></r>');
$xml->rewind(); var_dump($xml->hasChildren(), $xml->current()->count());
$source = $argc > 0 ? '$xml = new SimpleXMLElement("<r><i>1</i><i>2</i></r>"); $items = $xml->i; foreach ($items as $item) { echo $item, json_encode($items), ";"; }' : '';
eval($source);
} else {
$xml = new SimpleXMLElement('<r><child z="0"><item>0</item></child></r>');
emptyChains(null);
emptyChains($xml);
$ordinary = new stdClass(); $ordinary->child = null;
emptyChains($ordinary);
$code = $argc > 0 ? '$null = null; echo (int)empty($null?->child->{emptyChainName()}), "|", (int)empty($null?->child[emptyChainIndex()]), "|"; echo (int)empty($xml?->child->{emptyChainName()}), "|", (int)empty($xml?->child[emptyChainIndex()]), "|"; echo (int)empty($ordinary?->child->{emptyChainName()}), "|", (int)empty($ordinary?->child[emptyChainIndex()]), "|";' : '';
eval($code);
}
"####, &[&[], &["second-case"]]);
    assert_eq!(runs.len(), 2);
    // test_simplexml_projection_preserves_cursor_and_has_children_ignores_namespace
    {
        let output = &runs[0];
        assert!(output.success, "case=test_simplexml_projection_preserves_cursor_and_has_children_ignores_namespace stdout={:?} stderr={}", output.stdout, output.stderr);
        let out = output;
        assert_eq!(out.stdout, "[\"1\",\"2\",\"3\"]|1:1|1{\"0\":\"1\",\"1\":\"2\",\"2\":\"3\"};2{\"0\":\"1\",\"1\":\"2\",\"2\":\"3\"};3{\"0\":\"1\",\"1\":\"2\",\"2\":\"3\"};|bool(true)\nint(0)\n1{\"0\":\"1\",\"1\":\"2\"};2{\"0\":\"1\",\"1\":\"2\"};");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
    // test_simplexml_empty_long_nullsafe_chains
    {
        let output = &runs[1];
        assert!(output.success, "case=test_simplexml_empty_long_nullsafe_chains stdout={:?} stderr={}", output.stdout, output.stderr);
        let out = output;
        assert_eq!(out.stdout, "1|1|name|1|index|1|name|1|index|1|1|1|name|1|index|1|name|1|index|1|");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// Shares one native catalog compile across two independent process fixtures.
/// Original eval forms, caller scopes, outputs and ownership assertions are retained.
#[test]
fn test_simplexml_dynamic_property_and_write_fetch_bundle() {
    if skip_without_xml_native("test_simplexml_dynamic_property_and_write_fetch_bundle") { return; }
    let runs = compile_and_run_capture_per_argv(r####"<?php
class DynamicNameXml extends SimpleXMLElement {
    public string $a = 'slot';
    public function __get(string $name): mixed { return 'get-hook'; }
    public function __set(string $name, mixed $value): void { echo 'set-hook|'; }
    public function __isset(string $name): bool { return false; }
    public function __unset(string $name): void { echo 'unset-hook|'; }
}
function xmlPropertyName(string $name): string { echo $name, ':'; return $name; }
function mutateDynamicXml(mixed $node): void {
    echo (string)$node->{xmlPropertyName('a')}, '|';
    $node->{xmlPropertyName('b')} = 'two';
    var_dump(isset($node->{xmlPropertyName('b')}));
    echo $node->{xmlPropertyName('missing')} ?? 'fallback', '|';
    unset($node->{xmlPropertyName('a')});
    var_dump(isset($node->a));
}
function missingXmlWriteIndex(): int { global $xml; echo (int)isset($xml->positive), '|'; return 1; }
function missingXmlWriteValue(): string { global $xml; echo (int)isset($xml->positive), '|'; return 'ignored'; }
function missingEvalXmlWriteIndex(): int { global $xml; echo (int)isset($xml->evaluated), '|'; return 1; }
function missingEvalXmlWriteValue(): string { global $xml; echo (int)isset($xml->evaluated), '|'; return 'ignored'; }
if ($argc === 1) {
$xml = new DynamicNameXml('<r><a>native</a></r>');
$key = $argc > 0 ? 'a' : 'missing'; echo (string)$xml->{$key}, '|';
mutateDynamicXml($xml);
eval('$key = "b"; echo (string)$xml->{$key}, "|"; unset($xml->{$key}); var_dump(isset($xml->{$key}));');
} else {
$xml = new SimpleXMLElement('<r/>');
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$xml->positive[missingXmlWriteIndex()] = missingXmlWriteValue();
$xml->negative[-1] = 'ignored';
$xml->zero[0] = 'c';
$xml->appended[] = 'd';
$selection = $xml->absent;
$selection[0] = 'ignored'; $selection[1] = 'ignored'; $selection[] = 'ignored';
restore_error_handler();
echo $xml->asXML();
$code = $argc > 0 ? '$xml->evaluated[missingEvalXmlWriteIndex()] = missingEvalXmlWriteValue();' : '';
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
eval($code);
restore_error_handler();
echo count($xml->evaluated), ':', (string)$xml->evaluated;
}
"####, &[&[], &["second-case"]]);
    assert_eq!(runs.len(), 2);
    // test_simplexml_dynamic_property_names
    {
        let output = &runs[0];
        assert!(output.success, "case=test_simplexml_dynamic_property_names stdout={:?} stderr={}", output.stdout, output.stderr);
        let out = output.stdout.as_str();
        assert_eq!(out, "native|a:native|b:b:bool(true)\nmissing:fallback|a:bool(false)\ntwo|bool(false)\n");
    }
    // test_simplexml_missing_child_write_fetch
    {
        let output = &runs[1];
        assert!(output.success, "case=test_simplexml_missing_child_write_fetch stdout={:?} stderr={}", output.stdout, output.stderr);
        let out = output.stdout.as_str();
        assert_eq!(out, concat!(
            "0|0|2:main(): Cannot add element positive number 1 when only 0 such elements exist|",
            "2:main(): Cannot add element negative number -1 when only 0 such elements exist|",
            "<?xml version=\"1.0\"?>\n<r><positive/><negative/><zero>c</zero><appended>d</appended></r>\n",
            "0|0|2:Unknown: Cannot add element evaluated number 1 when only 0 such elements exist|1:",
        ));
    }
}
