//! Purpose:
//! Verifies native XML debug renderers use PHP property snapshots and preserve nested output.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on Linux and macOS.
//!
//! Key details:
//! - AOT and opaque eval share ordinary and recursive subclass debug fixtures.

use crate::support::*;

/// Native debug snapshots hide implementation fields, preserve capture prefixes, and retire owners.
#[test]
fn test_simplexml_native_debug_projection_bundle() {
    if skip_without_xml_native("test_simplexml_native_debug_projection_bundle") { return; }
    let definitions = r###"class CallbackXmlDebug extends SimpleXMLElement {
    public function __debugInfo(): ?array {
        var_dump('inner');
        $nested = print_r(['inside' => 7], true);
        return ['visible' => $nested, 'zero' => 0];
    }
}
class RecursiveXmlDebug extends SimpleXMLElement {
    public function __debugInfo(): ?array { return ['self' => $this]; }
}
"###;
    let body = r###"$xml = new SimpleXMLElement('<r a="1"><b>2</b><b>3</b></r>');
foreach ([$xml, $xml->children(), $xml->b] as $node) {
    echo json_encode(print_r($node, true)), "\n";
    ob_start(); var_dump($node); $dump = ob_get_clean();
    echo (int)str_contains($dump, 'object(SimpleXMLElement)#'), (int)str_contains($dump, 'string(1) "2"'), (int)!str_contains($dump, '__elephc_'), "\n";
    echo var_export($node, true), "\n";
}
$node = new CallbackXmlDebug('<r/>');
echo 'callbacks|', json_encode(print_r([$node, $node], true)), "\n";
ob_start(); var_dump([$node, $node]); $dump = ob_get_clean();
echo (int)str_contains($dump, 'string(5) "inner"'), (int)str_contains($dump, '["visible"]=>'), (int)!str_contains($dump, '__elephc_'), "\n";
$recursive = new RecursiveXmlDebug('<r/>');
echo json_encode(print_r([$recursive, $recursive], true)), "\n";
ob_start(); var_dump([$recursive, $recursive]); $dump = ob_get_clean();
echo (int)str_contains($dump, '*RECURSION*'), (int)!str_contains($dump, '__elephc_'), "\n";
unset($xml, $node, $recursive, $dump);
"###;
    let quoted_body = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php\n{definitions}\n{body}\n$source = $argc > 0 ? '{quoted_body}' : '';\neval($source);");
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let expected = r###""SimpleXMLElement Object\n(\n    [@attributes] => Array\n        (\n            [a] => 1\n        )\n\n    [b] => Array\n        (\n            [0] => 2\n            [1] => 3\n        )\n\n)\n"
111
\SimpleXMLElement::__set_state(array(
   '@attributes' =>
  array (
    'a' => '1',
  ),
   'b' =>
  array (
    0 => '2',
    1 => '3',
  ),
))
"SimpleXMLElement Object\n(\n    [@attributes] => Array\n        (\n            [a] => 1\n        )\n\n    [b] => Array\n        (\n            [0] => 2\n            [1] => 3\n        )\n\n)\n"
111
\SimpleXMLElement::__set_state(array(
   'b' =>
  array (
    0 => '2',
    1 => '3',
  ),
))
"SimpleXMLElement Object\n(\n    [0] => 2\n    [1] => 3\n)\n"
111
\SimpleXMLElement::__set_state(array(
   0 => '2',
   1 => '3',
))
callbacks|string(5) "inner"
string(5) "inner"
"Array\n(\n    [0] => CallbackXmlDebug Object\n        (\n            [visible] => Array\n(\n    [inside] => 7\n)\n\n            [zero] => 0\n        )\n\n    [1] => CallbackXmlDebug Object\n        (\n            [visible] => Array\n(\n    [inside] => 7\n)\n\n            [zero] => 0\n        )\n\n)\n"
111
"Array\n(\n    [0] => RecursiveXmlDebug Object\n        (\n            [self] => RecursiveXmlDebug Object\n *RECURSION*\n        )\n\n    [1] => RecursiveXmlDebug Object\n        (\n            [self] => RecursiveXmlDebug Object\n *RECURSION*\n        )\n\n)\n"
11
"###.replace(" =>\n", " => \n");
    assert_eq!(out.stdout, expected.repeat(2));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing debug hook is fatal even inside a PHP try block, matching PHP internals.
#[test]
fn test_simplexml_native_debug_hook_exception_is_fatal() {
    if skip_without_xml_native("test_simplexml_native_debug_hook_exception_is_fatal") { return; }
    let out = compile_and_run_capture(r#"<?php
class ThrowingXmlDebug extends SimpleXMLElement {
    public function __debugInfo(): ?array { throw new RuntimeException('debug'); }
}
try { echo print_r(new ThrowingXmlDebug('<r/>'), true); }
catch (Throwable $error) { echo 'caught-hook|'; }
echo 'after-hook|';
"#);
    assert!(!out.success);
    assert!(!out.stdout.contains("caught-hook|") && !out.stdout.contains("after-hook|"), "{}", out.stdout);
    assert!(out.stderr.contains("__debuginfo() must return an array"), "{}", out.stderr);
}
