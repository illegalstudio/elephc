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
class GuardedXmlDebug extends SimpleXMLElement {
    public function __debugInfo(): ?array { echo 'GUARD|'; var_dump($this); return ['visible' => 1]; }
}
class NullXmlDebug extends SimpleXMLElement {
    public function __debugInfo(): ?array { return null; }
}
class EchoXmlDebug extends SimpleXMLElement {
    public function __debugInfo(): ?array { echo "HOOK\n"; return ['v' => 1]; }
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
ob_start(); var_dump(new GuardedXmlDebug('<r/>')); $dump = ob_get_clean();
echo (int)str_contains($dump, '*RECURSION*'), (int)str_contains($dump, '["visible"]=>'), (int)(count(explode('GUARD|', $dump)) === 2), "\n";
$node = new NullXmlDebug('<r/>');
set_error_handler(function(int $level, string $message): bool {
    echo 'null:', $level, ':', (int)str_starts_with($message, 'Returning null from NullXmlDebug::__debugInfo()'), '|'; return true;
});
echo json_encode(print_r($node, true)), "\n";
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('null-handler'); });
try { print_r($node, true); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
restore_error_handler(); echo 'after-null', "\n";
unset($node, $dump);
$node = new CallbackXmlDebug('<r/>');
ob_start(); print_r(['k' => $node]); $echoed = ob_get_clean();
echo json_encode($echoed), "\n";
$flag = $argc < 0;
ob_start(); $result = print_r(['k' => $node], $flag); $echoed = ob_get_clean();
echo (int)($result === true), '|', json_encode($echoed), "\n";
ob_start(); var_dump(['k' => new EchoXmlDebug('<r/>')]); $dump = ob_get_clean();
echo (int)str_contains($dump, "[\"k\"]=>\n  HOOK\nobject(EchoXmlDebug)#"), "\n";
unset($node, $echoed, $result, $dump, $flag);
"###;
    let quoted_body = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php\n{definitions}\n{body}\n$source = $argc > 0 ? '{quoted_body}' : '';\neval($source);");
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(&source);
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
111
null:8192:1|"NullXmlDebug Object\n(\n)\n"
null-handler|after-null
"string(5) \"inner\"\nArray\n(\n    [k] => CallbackXmlDebug Object\n        (\n            [visible] => Array\n(\n    [inside] => 7\n)\n\n            [zero] => 0\n        )\n\n)\n"
1|"string(5) \"inner\"\nArray\n(\n    [k] => CallbackXmlDebug Object\n        (\n            [visible] => Array\n(\n    [inside] => 7\n)\n\n            [zero] => 0\n        )\n\n)\n"
1
"###.replace(" =>\n", " => \n");
    assert_eq!(out.stdout, expected.repeat(2));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\nDebug composite assembly:\n{}", out.stderr, selected_function_assembly(&assembly, &["_main".into()]));
}

/// Throwing debug hooks bypass user handlers and remain fatal in AOT and dynamic eval.
#[test]
fn test_simplexml_native_debug_hook_exception_is_fatal_bundle() {
    if skip_without_xml_native("test_simplexml_native_debug_hook_exception_is_fatal_bundle") { return; }
    for opaque_eval in [false, true] {
        let error_class = if opaque_eval { "EvalXmlDebugError" } else { "RuntimeException" };
        let error_declaration = if opaque_eval { "class EvalXmlDebugError extends RuntimeException {}" } else { "" };
        let body = format!(r#"{error_declaration}
class ThrowingXmlDebug extends SimpleXMLElement {{
    public function __debugInfo(): ?array {{ throw new {error_class}('debug'); }}
}}
set_error_handler(function(int $level, string $message): bool {{ echo 'unexpected-handler|'; return true; }});
try {{
    if ($argc === 1) {{ print_r(new ThrowingXmlDebug('<r/>')); }}
    else {{ var_dump(new ThrowingXmlDebug('<r/>')); }}
}} catch (Throwable $error) {{ echo 'caught-hook|'; }}
echo 'after-hook|';
"#);
        let source = if opaque_eval {
            let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
            format!("<?php $anchor = new SimpleXMLElement('<r/>'); $source = $argc > 0 ? '{quoted}' : ''; eval($source);")
        } else { format!("<?php {body}") };
        let runs = compile_and_run_capture_per_argv(&source, &[&[], &["x"]]);
        for (operation, out) in ["print_r", "var_dump"].into_iter().zip(runs) {
            assert!(!out.success, "{operation} eval={opaque_eval}");
            assert!(!out.stdout.contains("unexpected-handler|") && !out.stdout.contains("caught-hook|") && !out.stdout.contains("after-hook|"), "{}", out.stdout);
            assert!(out.stderr.contains(&format!("Warning: Uncaught {error_class}: debug")), "{}", out.stderr);
            assert!(out.stderr.contains("\n  thrown in ") && out.stderr.contains(" on line "), "{}", out.stderr);
            assert!(out.stderr.contains("__debuginfo() must return an array"), "{}", out.stderr);
        }
    }
}

/// Classes declared inside opaque eval use their own debug hook and diagnostic class name.
#[test]
fn test_simplexml_eval_declared_debug_overrides_bundle() {
    if skip_without_xml_native("test_simplexml_eval_declared_debug_overrides_bundle") { return; }
    let body = r###"class EvalXmlDebug extends SimpleXMLElement {
    public function __debugInfo(): ?array { return [0 => 'zero', 'k' => 1]; }
}
class EvalGuardedXmlDebug extends SimpleXMLElement {
    public function __debugInfo(): ?array { echo 'GUARD|'; var_dump($this); return ['g' => 1]; }
}
class EvalNullXmlDebug extends SimpleXMLElement {
    public function __debugInfo(): ?array { return null; }
}
$node = new EvalXmlDebug('<r><ignored>parent</ignored></r>');
echo json_encode(print_r($node, true)), "\n";
ob_start(); var_dump($node); $dump = ob_get_clean();
echo (int)str_contains($dump, 'object(EvalXmlDebug)#'), (int)str_contains($dump, "[0]=>\n"), (int)str_contains($dump, '["k"]=>'), "\n";
ob_start(); var_dump(new EvalGuardedXmlDebug('<r/>')); $dump = ob_get_clean();
echo (int)(count(explode('GUARD|', $dump)) === 2), (int)str_contains($dump, '*RECURSION*'), "\n";
$node = new EvalNullXmlDebug('<r/>');
set_error_handler(function(int $level, string $message): bool {
    echo 'null:', $level, ':', (int)str_starts_with($message, 'Returning null from EvalNullXmlDebug::__debugInfo()'), '|'; return true;
});
echo json_encode(print_r($node, true)), "\n";
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('eval-null'); });
try { print_r($node, true); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
restore_error_handler(); echo 'after', "\n";
unset($node, $dump);
"###;
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $anchor = new SimpleXMLElement('<r/>'); $source = $argc > 0 ? '{quoted}' : ''; eval($source); unset($anchor);");
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(&source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, r###""EvalXmlDebug Object\n(\n    [0] => zero\n    [k] => 1\n)\n"
111
11
null:8192:1|"EvalNullXmlDebug Object\n(\n)\n"
eval-null|after
"###);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\nDebug composite assembly:\n{}", out.stderr, selected_function_assembly(&assembly, &["_main".into()]));
}
