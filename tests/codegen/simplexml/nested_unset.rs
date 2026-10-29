//! Purpose:
//! Covers erased nested dimension receivers whose current value is a native XML object.
//!
//! Called from:
//! - Managed XML executable codegen CI.
//!
//! Key details:
//! - The final key may replace the parent array before its object element is selected.

use crate::support::*;

/// AOT and eval select the current nested object after key effects replace its parent array.
#[test]
fn test_simplexml_nested_storage_unset_selects_after_key_bundle() {
    if skip_without_xml_native("test_simplexml_nested_storage_unset_selects_after_key_bundle") { return; }
    let source = r#"<?php
function replaceNestedStorageEntries(SimpleXMLElement $replacement): string {
    global $nestedStorageEntries;
    echo 'key|';
    $nestedStorageEntries = [$replacement];
    return 'id';
}
$old = new SimpleXMLElement('<old id="old"/>');
$replacement = new SimpleXMLElement('<new id="new"/>');
$nestedStorageEntries = [$old];
unset($nestedStorageEntries[0][replaceNestedStorageEntries($replacement)]);
echo $old->asXML(), $replacement->asXML();
unset($old, $replacement, $nestedStorageEntries);
"#;
    let renamed = source.replace("replaceNestedStorageEntries", "replaceEvalNestedStorageEntries");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $activate = new SimpleXMLElement('<activate/>'); unset($activate); $source = $argc > 0 ? '{body}' : ''; eval($source); unset($source);");
    let expected = "key|<?xml version=\"1.0\"?>\n<old id=\"old\"/>\n<?xml version=\"1.0\"?>\n<new/>\n";
    assert_bundle_with(&[
        BundleCase { name: "nested_unset_aot", source, expected },
        BundleCase { name: "nested_unset_eval", source: &eval, expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}
