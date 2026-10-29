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
unset($view, $object, $iterator);
' : '';
eval($source);
unset($xml, $source);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "8:Indirect modification of overloaded element of SimpleXMLElement has no effect|",
        "<?xml version=\"1.0\"?>\n<r/>\n",
        "{\"k\":{\"x\":2,\"0\":3}}|{\"k\":{\"x\":5}}|",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
