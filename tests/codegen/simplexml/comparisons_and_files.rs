//! Purpose:
//! Covers native SimpleXML comparisons, default namespaces and file-loader diagnostics.
//!
//! Called from:
//! - Managed-libxml2 codegen CI and optional PHP reference checks.
//!
//! Key details:
//! - Bundles AOT and runtime eval while heap-debug checks temporary owners and handler throws.

use crate::support::*;

/// Native node identity and peer-directed casts preserve PHP comparison and namespace behavior.
#[test]
fn test_simplexml_native_comparison_and_default_namespace_bundle() {
    if skip_without_xml_native("test_simplexml_native_comparison_and_default_namespace_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
$default = new SimpleXMLElement('<r xmlns="urn:def"><child>text</child></r>');
echo count($default->child), ':', count($default), ':', count($default->children()), ':', count($default->children('urn:def')), ':';
foreach ($default as $item) { echo $item->getName(), ','; }
echo '|', json_encode($default), '|';
function compareNativeXml(mixed $left, mixed $right): void {
    echo (int)($left == $right), (int)($left < $right), (int)($left > $right), (int)($left <= $right), (int)($left >= $right), ':', $left <=> $right, '|';
}
$xml = new SimpleXMLElement('<r>2</r>');
echo (int)($xml == '2'), (int)($xml == 2), (int)($xml == 1), (int)($xml > 1), (int)($xml > 'a'), '|';
foreach (['2', 2, 1, 3.0, 'a', false, true, null, [], new stdClass(), NAN] as $peer) {
    compareNativeXml($xml, $peer); compareNativeXml($peer, $xml);
}
$empty = (new SimpleXMLElement('<r><e/></r>'))->e[0];
compareNativeXml($empty, false);
compareNativeXml(new SimpleXMLElement('<r>0</r>'), true);
$fraction = new SimpleXMLElement('<r>3.5</r>');
compareNativeXml($fraction, 3); compareNativeXml($fraction, 3.0);
class ComparisonXmlText extends SimpleXMLElement { public function __toString(): string { return '99'; } }
$custom = new ComparisonXmlText('<r>2</r>');
compareNativeXml($custom, 2); compareNativeXml($custom, '99');
$tree = new SimpleXMLElement('<r><a>1</a><a>2</a><b>1</b></r>');
compareNativeXml($tree, $tree->a); compareNativeXml($tree->a, $tree->b);
compareNativeXml($tree->a, $tree->a[0]);
compareNativeXml($tree, new SimpleXMLElement('<r><a>1</a><a>2</a><b>1</b></r>'));
$saved = $tree->a->missing; $first = $tree->a[0]; unset($tree->a[0]);
compareNativeXml($saved, $first);
$doc = new DOMDocument(); $doc->loadXML('<r/>');
compareNativeXml(simplexml_import_dom($doc), simplexml_import_dom($doc));
$source = $argc > 0 ? '$ex = new SimpleXMLElement("<r>3.5</r>"); compareNativeXml($ex, 3); compareNativeXml($ex, 3.0); compareNativeXml($ex, "3.5"); $ee = (new SimpleXMLElement("<r><e/></r>"))->e[0]; compareNativeXml($ee, false);' : '';
eval($source);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(out.stdout, r###"1:1:1:1:child,|{"child":"text"}|11010|10011:0|10011:0|10011:0|10011:0|00101:1|01010:-1|01010:-1|00101:1|01010:-1|00101:1|00101:1|01010:-1|10011:0|10011:0|00101:1|01010:-1|00101:1|01010:-1|00000:1|00000:1|00000:1|00000:1|10011:0|10011:0|10011:0|00101:1|10011:0|10011:0|10011:0|10011:0|00000:1|00000:1|10011:0|10011:0|10011:0|00101:1|10011:0|10011:0|"###);
}

/// File failures retain caller warnings and constructor chains while DOM commits successful parses before throws.
#[test]
fn test_simplexml_file_loader_and_dom_throw_state_bundle() {
    if skip_without_xml_native("test_simplexml_file_loader_and_dom_throw_state_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
set_error_handler(function(int $level, string $message): bool {
    if (str_starts_with($message, 'simplexml_load_file(): I/O warning : ')) { echo $level, ':file-io|'; }
    elseif (str_starts_with($message, 'SimpleXMLElement::__construct(): I/O warning : ')) { echo $level, ':ctor-io|'; }
    else { echo 'unexpected:', $message, '|'; }
    return true;
});
$missing = __DIR__ . '/simplexml-missing-input.xml';
var_dump(simplexml_load_file($missing));
try { new SimpleXMLElement($missing, 0, true); } catch (Exception $error) { echo $error->getMessage(), '|'; }
var_dump(simplexml_load_file(''));
var_dump(simplexml_load_file($missing, null, LIBXML_NOWARNING));
try { simplexml_load_file("bad\0path"); } catch (ValueError $error) { echo $error->getMessage(), '|'; }
$path = __DIR__ . '/simplexml-existing-input.xml'; file_put_contents($path, '<r><v>loaded</v></r>');
$loaded = simplexml_load_file($path); $constructed = new SimpleXMLElement($path, 0, true);
echo (string)$loaded->v, ':', (string)$constructed->v, '|'; unlink($path);
$source = $argc > 0 ? 'var_dump(simplexml_load_file($missing)); try { new SimpleXMLElement($missing, 0, true); } catch (Exception $error) { echo $error->getMessage(), "|"; }' : '';
eval($source);
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('caught'); });
try { simplexml_load_file($missing); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
try { new SimpleXMLElement($missing, 0, true); } catch (Exception $error) { echo get_class($error), ':', $error->getMessage(), ':', get_class($error->getPrevious()), '|'; }
try { new SimpleXMLElement('<broken>'); } catch (Exception $error) { echo get_class($error), ':', $error->getMessage(), ':', get_class($error->getPrevious()), '|'; }
$doc = new DOMDocument(); $doc->loadXML('<old/>');
try { $doc->loadXML('<?xml version="1.1"?><new/>'); } catch (RuntimeException $error) { echo $error->getMessage(), ':', $doc->documentElement->nodeName, '|'; }
try { $doc->loadXML('<broken>'); } catch (RuntimeException $error) { echo $error->getMessage(), ':', $doc->documentElement->nodeName, '|'; }
restore_error_handler();
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(out.stdout, r###"2:file-io|bool(false)
2:ctor-io|String could not be parsed as XML|2:file-io|bool(false)
bool(false)
simplexml_load_file(): Argument #1 ($filename) must not contain any null bytes|loaded:loaded|2:file-io|bool(false)
2:ctor-io|String could not be parsed as XML|caught|Exception:String could not be parsed as XML:RuntimeException|Exception:String could not be parsed as XML:RuntimeException|caught:new|caught:new|"###);
}

