//! Purpose:
//! Isolates native XML debug projections, callbacks, captures and diagnostics under heap checks.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on every executable target.
//!
//! Key details:
//! - One opaque-eval catalog is compiled once and every argv case must retire all owners.

use crate::support::*;

/// Separates scalar projections, recursive hooks, handler throws and ordinary capture controls.
#[test]
fn test_simplexml_debug_projection_owners_isolated_bundle() {
    if skip_without_xml_native("test_simplexml_debug_projection_owners_isolated_bundle") { return; }
    let runs = compile_and_run_with_heap_debug_per_argv(r###"<?php
class OwnerCallbackXmlDebug extends SimpleXMLElement {
    public function __debugInfo(): ?array { var_dump('inner'); return ['visible' => print_r(['inside' => 7], true), 'zero' => 0]; }
}
class OwnerRecursiveXmlDebug extends SimpleXMLElement { public function __debugInfo(): ?array { return ['self' => $this]; } }
class OwnerGuardedXmlDebug extends SimpleXMLElement { public function __debugInfo(): ?array { echo 'GUARD|'; var_dump($this); return ['visible' => 1]; } }
class OwnerNullXmlDebug extends SimpleXMLElement { public function __debugInfo(): ?array { return null; } }
class OwnerPlainDebug { public function __debugInfo(): ?array { return ['visible' => 1]; } }
$xml = new SimpleXMLElement('<r id="1"><b>2</b><b>3</b></r>');
if ($argc === 1) { echo strlen(print_r($xml, true)); }
elseif ($argc === 2) { ob_start(); var_dump($xml); $dump = ob_get_clean(); echo (int)str_contains($dump, 'string(1) "2"'); }
elseif ($argc === 3) { echo strlen(var_export($xml, true)); }
elseif ($argc === 4) { echo json_encode($xml); }
elseif ($argc === 5) { echo strlen(print_r(new OwnerCallbackXmlDebug('<r/>'), true)); }
elseif ($argc === 6) { echo strlen(print_r(new OwnerRecursiveXmlDebug('<r/>'), true)); }
elseif ($argc === 7) {
    ob_start(); var_dump(new OwnerGuardedXmlDebug('<r/>')); $dump = ob_get_clean();
    echo (int)str_contains($dump, '*RECURSION*'), (int)str_contains($dump, '["visible"]=>'), (int)(count(explode('GUARD|', $dump)) === 2);
} elseif ($argc === 8) {
    set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
    echo strlen(print_r(new OwnerNullXmlDebug('<r/>'), true)); restore_error_handler();
} elseif ($argc === 9) {
    set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('null-handler'); });
    try { print_r(new OwnerNullXmlDebug('<r/>'), true); } catch (RuntimeException $error) { echo $error->getMessage(); }
    restore_error_handler();
} elseif ($argc === 10) {
    ob_start(); var_dump(new OwnerCallbackXmlDebug('<r/>')); $dump = ob_get_clean();
    echo (int)str_contains($dump, 'string(5) "inner"'), (int)str_contains($dump, '["visible"]=>');
} elseif ($argc === 11) {
    $flag = $argc < 0; ob_start(); $result = print_r(['k' => new OwnerCallbackXmlDebug('<r/>')], $flag); $dump = ob_get_clean();
    echo (int)($result === true), '|', strlen($dump);
} elseif ($argc === 12) { echo strlen(print_r(['inside' => 7], true)); }
elseif ($argc === 13) {
    $source = $argc > 0 ? 'class EvalOwnerXmlDebug extends SimpleXMLElement { public function __debugInfo(): ?array { return [0 => "zero", "k" => 1]; } } echo strlen(print_r(new EvalOwnerXmlDebug("<r/>"), true));' : '';
    eval($source);
} elseif ($argc === 14) {
    $source = $argc > 0 ? 'class EvalOwnerNullXmlDebug extends SimpleXMLElement { public function __debugInfo(): ?array { return null; } } set_error_handler(function(int $level, string $message): bool { throw new RuntimeException("eval-null"); }); try { print_r(new EvalOwnerNullXmlDebug("<r/>"), true); } catch (RuntimeException $error) { echo $error->getMessage(); } restore_error_handler();' : '';
    eval($source);
} elseif ($argc === 15) { echo json_encode(print_r($xml, true)); }
else { echo strlen(print_r(new OwnerPlainDebug(), true)); }
"###, &[
        &[],
        &["x"],
        &["x", "x"],
        &["x", "x", "x"],
        &["x", "x", "x", "x"],
        &["x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x"],
    ]);
    let expected = ["178", "1", "147", "{\"@attributes\":{\"id\":\"1\"},\"b\":[\"2\",\"3\"]}", "string(5) \"inner\"\n95", "91", "111", "8192|29", "null-handler", "11", "1|167", "28", "58", "eval-null", "\"SimpleXMLElement Object\\n(\\n    [@attributes] => Array\\n        (\\n            [id] => 1\\n        )\\n\\n    [b] => Array\\n        (\\n            [0] => 2\\n            [1] => 3\\n        )\\n\\n)\\n\"", "46"];
    assert_eq!(runs.len(), expected.len());
    let mut failures = Vec::new();
    for (mode, (out, expected)) in runs.iter().zip(expected).enumerate() {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("mode={mode} success={} stdout={:?} expected={expected:?}\n{}", out.success, out.stdout, out.stderr));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
