//! Purpose:
//! Isolates owners retained by native XML dimension writes and receiver rebindings.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on every executable target.
//!
//! Key details:
//! - One compilation separates each write surface while retaining opaque eval metadata.

use crate::support::*;

/// A caught dimension Error retires the unboxed receiver without consuming its live local.
#[test]
fn test_simplexml_nullable_property_receiver_survives_caught_dimension_error() {
    if skip_without_xml_native("test_simplexml_nullable_property_receiver_survives_caught_dimension_error") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
$xml = new SimpleXMLElement('<r><item>first</item></r>');
try { $xml->item[null] = 'wrong'; }
catch (ValueError $error) { echo $xml->getName(), ':', (string)$xml->item; }
$xml = null;
echo ':', (int)($xml === null);
unset($xml, $error);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "r:first:1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Final nullable storage releases the temporary object extracted for an ordinary append.
#[test]
fn test_simplexml_append_from_nullable_local_releases_unboxed_receiver() {
    if skip_without_xml_native("test_simplexml_append_from_nullable_local_releases_unboxed_receiver") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
function freshNullableAppendValue(): mixed { return str_repeat('added', 1); }
$xml = new SimpleXMLElement('<r><item>first</item></r>');
$view = $xml->item;
$view[] = freshNullableAppendValue();
$view = null;
echo (string)$xml->item[1], ':', (int)($view === null);
unset($xml, $view);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "added:1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Separates throwing offsets, append receivers, fresh values and closure rebindings.
#[test]
fn test_simplexml_dimension_write_owners_isolated_bundle() {
    if skip_without_xml_native("test_simplexml_dimension_write_owners_isolated_bundle") { return; }
    let args: Vec<_> = (0..22).map(|count| vec!["x"; count]).collect();
    let argvs: Vec<_> = args.iter().map(Vec::as_slice).collect();
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(r#"<?php
function appendIsolatedXmlMixed(mixed $view): void { $view[] = 'a'; }
function freshIsolatedXmlString(): string { return str_repeat('x', 3); }
function freshIsolatedXmlMixed(string $value): mixed { return str_repeat($value, 1); }
function isolatedGlobalXmlKey(): int { global $isolatedGlobalXml; echo (int)isset($isolatedGlobalXml->item), '|'; return 1; }
function isolatedGlobalXmlRhs(): int { global $isolatedGlobalXml; echo (int)isset($isolatedGlobalXml->item), '|'; return 1; }
class IsolatedXmlAppendStatic { public static mixed $view; }
class IsolatedXmlTimingStatic { public static SimpleXMLElement $xml; }
$xml = new SimpleXMLElement('<r><item>keep</item></r>');
if ($argc === 1) { echo $xml->getName(); }
elseif ($argc === 2) {
    $node = $xml->item[0];
    try { $node[null] = 'wrong'; } catch (ValueError $error) { echo 'null'; }
} elseif ($argc === 3) {
    try { $xml[null] = 'wrong'; } catch (ValueError $error) { echo 'null'; }
} elseif ($argc === 4) {
    try { $xml->item[null] = 'wrong'; } catch (ValueError $error) { echo 'null'; }
} elseif ($argc === 5) {
    $node = $xml->item[0]; $node[] = 'a'; echo (string)$xml->item;
} elseif ($argc === 6) {
    appendIsolatedXmlMixed($xml->item); echo (string)$xml->item[1];
} elseif ($argc === 7) {
    $holder = new stdClass(); $holder->view = $xml->item; $holder->view[] = 'a';
    echo (string)$xml->item[1];
} elseif ($argc === 8) {
    $parts = [$xml->item]; $parts[0][] = 'a'; echo (string)$xml->item[1];
} elseif ($argc === 9) {
    IsolatedXmlAppendStatic::$view = $xml->item; IsolatedXmlAppendStatic::$view[] = 'a';
    echo (string)$xml->item[1];
} elseif ($argc === 10) {
    $attributes = $xml->attributes();
    try { $attributes[] = freshIsolatedXmlString(); } catch (Error $error) { echo 'append'; }
} elseif ($argc === 11) {
    $attributes = $xml->attributes();
    try { $attributes[] = freshIsolatedXmlMixed('x'); } catch (Error $error) { echo 'append'; }
} elseif ($argc === 12) {
    $source = $argc > 0 ? '$view = $xml->item; $view[] = "a"; echo (string)$xml->item[1];' : '';
    eval($source);
} elseif ($argc === 13) {
    $tail = $xml->item; $tail[] = freshIsolatedXmlMixed('g'); echo (string)$xml->item[1];
} elseif ($argc === 14) {
    IsolatedXmlAppendStatic::$view = $xml->item;
    IsolatedXmlAppendStatic::$view[] = freshIsolatedXmlMixed('h'); echo (string)$xml->item[1];
} elseif ($argc === 15) {
    $xml = new SimpleXMLElement('<r/>');
    set_error_handler(function(int $level, string $message): bool { return true; });
    $xml->item[1] += 1; restore_error_handler(); echo count($xml->item), ':', (string)$xml->item;
} elseif ($argc === 16) {
    $xml = new SimpleXMLElement('<old/>'); $old = $xml;
    $xml->item[0] = (function() use (&$xml): string { $xml = new SimpleXMLElement('<new/>'); return 'v'; })();
    echo $old->getName(), ':', (int)isset($old->item), '|', $xml->getName(), ':', (string)$xml->item;
} elseif ($argc === 17) {
    $xml = new SimpleXMLElement('<old><item>1</item></old>'); $old = $xml;
    $xml->item[0] += (function() use (&$xml): int { $xml = new SimpleXMLElement('<new><item>5</item></new>'); return 1; })();
    echo (string)$old->item, ':', (string)$xml->item;
} elseif ($argc === 18) {
    IsolatedXmlTimingStatic::$xml = new SimpleXMLElement('<sold/>'); $old = IsolatedXmlTimingStatic::$xml;
    IsolatedXmlTimingStatic::$xml->item[0] = (function(): string { IsolatedXmlTimingStatic::$xml = new SimpleXMLElement('<snew/>'); return 's'; })();
    echo $old->getName(), ':', (int)isset($old->item), '|', IsolatedXmlTimingStatic::$xml->getName(), ':', (string)IsolatedXmlTimingStatic::$xml->item;
} elseif ($argc === 19) {
    $number = 1; $change = function() use (&$number): void { $number = 2; }; $change(); echo $number;
} elseif ($argc === 20) {
    $source = $argc > 0 ? '$view = new SimpleXMLElement("<old/>"); $oldView = $view; $view->item[0] = (function() use (&$view): string { $view = new SimpleXMLElement("<new/>"); return "v"; })(); echo $oldView->getName(), ":", (int)isset($oldView->item), "|", $view->getName(), ":", (string)$view->item;' : '';
    eval($source);
} elseif ($argc === 21) {
    $isolatedGlobalXml = new SimpleXMLElement('<r/>');
    set_error_handler(function(int $level, string $message): bool { return true; });
    $isolatedGlobalXml->item[isolatedGlobalXmlKey()] += isolatedGlobalXmlRhs();
    restore_error_handler(); echo count($isolatedGlobalXml->item), ':', (string)$isolatedGlobalXml->item;
} else {
    $source = $argc > 0 ? '$evalGlobalOwnerXml = new SimpleXMLElement("<r/>"); function evalGlobalOwnerKey(): int { global $evalGlobalOwnerXml; echo (int)isset($evalGlobalOwnerXml->item), "|"; return 1; } function evalGlobalOwnerRhs(): int { global $evalGlobalOwnerXml; echo (int)isset($evalGlobalOwnerXml->item), "|"; return 1; } set_error_handler(function(int $level, string $message): bool { return true; }); $evalGlobalOwnerXml->item[evalGlobalOwnerKey()] += evalGlobalOwnerRhs(); restore_error_handler(); echo count($evalGlobalOwnerXml->item), ":", (string)$evalGlobalOwnerXml->item;' : '';
    eval($source);
}
"#, &argvs);
    let expected = ["r", "null", "null", "null", "a", "a", "a", "a", "a", "append", "append", "a", "g", "h", "1:", "old:0|new:v", "1:6", "sold:0|snew:s", "2", "old:0|new:v", "0|0|1:", "0|0|1:"];
    assert_eq!(runs.len(), expected.len());
    let mut failures = Vec::new();
    for (mode, (out, expected)) in runs.iter().zip(expected).enumerate() {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("mode={mode} success={} stdout={:?} expected={expected:?}\n{}", out.success, out.stdout, out.stderr));
        }
    }
    let mut symbols = vec!["_main".into(),
        elephc::names::method_symbol("SimpleXMLElement", "__elephc_append"),
        elephc::names::method_symbol("SimpleXMLElement", "__elephc_property_write_fetch"),
        elephc::names::method_symbol("SimpleXMLElement", "__elephc_dimension_set"),
        elephc::names::method_symbol("SimpleXMLElement", "__elephc_dimension_get"),
        elephc::names::method_symbol("SimpleXMLElement", "__elephc_number"),
        elephc::names::static_method_symbol("SimpleXMLElement", "__elephc_add"),
        elephc::names::function_symbol("freshIsolatedXmlMixed"),
    ];
    symbols.extend((0..5).map(|index|
        elephc::names::function_symbol(&format!("__eir_closure_main_{index}"))));
    assert!(failures.is_empty(), "{}\nDimension owner assembly:\n{}", failures.join("\n"), selected_function_assembly(&assembly, &symbols));
}
