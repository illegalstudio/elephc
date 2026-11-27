//! Purpose:
//! Keeps ordinary boxed array receivers alive when native XML append dispatch is available.
//!
//! Called from:
//! - The runtime codegen suite on every executable target.
//!
//! Key details:
//! - XML metadata alone activates the dispatch, including in unrelated callbacks.
//! - Global cells remain owned by their binding while appended values retain independent owners.

use crate::support::{compile_and_run_with_heap_debug, skip_without_xml_native};

/// XML capability discovery must not release an ordinary global array after append.
#[test]
fn test_xml_metadata_mixed_global_append_preserves_receiver() {
    if skip_without_xml_native("test_xml_metadata_mixed_global_append_preserves_receiver") { return; }
    let output = compile_and_run_with_heap_debug(r#"<?php
class AppendXmlMetadata extends SimpleXMLElement {}
function appendOpenTrace(): array { return array(); }
$trace = appendOpenTrace();
function appendRecord(string $tag, array $data = array()): void {
    global $trace;
    $trace[] = array($tag, $data);
}
$f = static function(): void { appendRecord('closure', array()); };
call_user_func_array($f, array());
appendRecord('after', array(1, 2));
echo json_encode($trace, JSON_THROW_ON_ERROR), chr(10);
unset($trace, $f);
"#);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "[[\"closure\",[]],[\"after\",[1,2]]]\n");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
