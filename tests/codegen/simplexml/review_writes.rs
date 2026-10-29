//! Purpose:
//! Covers indirect eval dimension writes after native selection and ordinary overloading.
//!
//! Called from:
//! - Managed XML codegen CI on every executable target.
//!
//! Key details:
//! - Disposable ArrayAccess copies never write back; native SPL dimensions retain write-through.

use crate::support::*;

/// Failed native selections discard nested array replacements while SPL containers remain writable.
#[test]
fn test_simplexml_eval_indirect_dimension_copies_bundle() {
    if skip_without_xml_native("test_simplexml_eval_indirect_dimension_copies_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
$xml = new SimpleXMLElement('<r/>');
if ($argc < 0) {
    $keepObject = new ArrayObject(['k' => ['x' => 1]]);
    $keepIterator = new ArrayIterator(['k' => ['x' => 1]]);
    echo json_encode($keepObject->getArrayCopy()), json_encode($keepIterator->getArrayCopy());
}
class NativeVirtualSetDimension extends ArrayObject {
    public function offsetSet(mixed $key, mixed $value): void { echo 'bad-native-set|'; parent::offsetSet($key, $value); }
}
class NativeReferenceDimension implements ArrayAccess {
    public mixed $slot = [];
    public function &offsetGet(mixed $key): mixed { return $this->slot; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetExists(mixed $key): bool { return false; }
    public function offsetUnset(mixed $key): void {}
}
$source = $argc > 0 ? '
set_error_handler(function(int $level, string $message): bool { echo $level, ":", $message, "|"; return true; });
$view = $xml->a;
$view[0][] = "discarded";
restore_error_handler();
echo $xml->asXML();
$object = new ArrayObject(["k" => ["x" => 1]]);
$object["k"]["x"] = 2;
$object["k"][] = 3;
echo json_encode($object->getArrayCopy()), "|";
$iterator = new ArrayIterator(["k" => ["x" => 1]]);
$iterator["k"]["x"] += 4;
echo json_encode($iterator->getArrayCopy()), "|";
class InheritedDimensionBag extends ArrayObject {}
class VirtualSetDimensionBag extends ArrayObject {
    public function offsetSet(mixed $key, mixed $value): void { echo "bad-virtual-set|"; parent::offsetSet($key, $value); }
}
$inherited = new InheritedDimensionBag(["k" => ["x" => 1]]);
$inherited["k"]["x"] = 6;
echo json_encode($inherited->getArrayCopy()), "|";
$virtual = new VirtualSetDimensionBag(["k" => ["x" => 1], "n" => []]);
$virtual["k"]["x"] = 7;
$virtual["n"][] = 8;
echo json_encode($virtual->getArrayCopy()), "|";
class ReferenceDimension implements ArrayAccess {
    public mixed $slot = [];
    public function &offsetGet(mixed $key): mixed { echo "bad-reference-get|"; return $this->slot; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetExists(mixed $key): bool { return false; }
    public function offsetUnset(mixed $key): void {}
}
$reference = new ReferenceDimension();
try { $reference[0][] = 1; } catch (Error $error) { echo $error->getMessage(), "|"; }
$nativeReference = new NativeReferenceDimension();
try { $nativeReference[0][] = 1; } catch (Error $error) { echo $error->getMessage(), "|"; }
$nativeVirtual = new NativeVirtualSetDimension(["k" => ["x" => 1]]);
try { $nativeVirtual["k"]["x"] = 9; } catch (Error $error) { echo $error->getMessage(), "|"; }
echo json_encode($nativeVirtual->getArrayCopy()), "|";
unset($view, $object, $iterator, $inherited, $virtual, $reference, $nativeReference, $nativeVirtual, $error);
' : '';
eval($source);
unset($xml, $source);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "8:Indirect modification of overloaded element of SimpleXMLElement has no effect|",
        "<?xml version=\"1.0\"?>\n<r/>\n",
        "{\"k\":{\"x\":2,\"0\":3}}|{\"k\":{\"x\":5}}|",
        "{\"k\":{\"x\":6}}|{\"k\":{\"x\":7},\"n\":[8]}|",
        "Unsupported dimension write: by-reference offsetGet|",
        "Unsupported dimension write: by-reference offsetGet|",
        "Unsupported dimension write: native SPL offsetSet override|{\"k\":{\"x\":1}}|",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nullable native reads warn and return null while silent coalesce bypasses public ArrayAccess.
#[test]
fn test_simplexml_nullable_native_subscripts_preserve_null_reads() {
    if skip_without_xml_native("test_simplexml_nullable_native_subscripts_preserve_null_reads") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
class NullableReadDimension extends SimpleXMLElement implements ArrayAccess {
    public function offsetGet(mixed $key): mixed { echo 'bad-public-get|'; return 'public'; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetExists(mixed $key): bool { return false; }
    public function offsetUnset(mixed $key): void {}
}
function nullableReadDimension(?NullableReadDimension $node): mixed { return $node[0]; }
function nullableCoalesceDimension(?NullableReadDimension $node): mixed { return $node[0] ?? 'absent'; }
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
var_dump(nullableReadDimension(null));
echo nullableCoalesceDimension(null), '|';
$node = new NullableReadDimension('<r>native</r>');
echo (string)nullableReadDimension($node), '|';
restore_error_handler();
unset($node);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2:Trying to access array offset on null|NULL\nabsent|native|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
