//! Purpose:
//! Isolates ordinary receiver owners used by native XML update programs.
//!
//! Called from:
//! - Managed-libxml2 executable CI through the SimpleXML test module.
//!
//! Key details:
//! - Independent heaps distinguish name/coalesce captures from ArrayAccess results.
//! - The original combined updates corpus keeps all its assertions.

use crate::support::*;

/// Captured names and lazy property writes retire their selected ordinary child and hash.
#[test]
fn test_simplexml_capture_coalesce_owners_bundle() {
    if skip_without_xml_native("test_simplexml_capture_coalesce_owners_bundle") { return; }
    let source = r#"<?php
class CoalesceOwnerXmlCapability extends SimpleXMLElement {}
class CoalesceOwnerChild { public mixed $value = 1; }
class CoalesceOwnerName {
    public function __toString(): string { echo 'name|'; return 'child'; }
    public function __destruct() { echo 'drop|'; }
}
function coalesceOwnerName(): CoalesceOwnerName { return new CoalesceOwnerName(); }
function coalesceOwnerValue(): int { echo 'value|'; return 2; }
function coalesceOwnerProgram(): void {
    $root = new stdClass(); $root->child = new CoalesceOwnerChild();
    $root->{coalesceOwnerName()}->value = coalesceOwnerValue();
    $root->{coalesceOwnerName()}->value += 1;
    $root->{coalesceOwnerName()}->value++;
    $root->{coalesceOwnerName()}->value ??= coalesceOwnerValue();
    unset($root->child->value);
    $root->{coalesceOwnerName()}->value ??= coalesceOwnerValue();
    for ($i = 0; $i < 2; $i++) { $root->{coalesceOwnerName()}->value += 1; }
    echo $root->child->value, '|';
    unset($root);
}
coalesceOwnerProgram();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!("value|name|drop|name|drop|name|drop|name|drop|",
        "name|value|name|drop|name|drop|name|drop|4|"));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
}

/// An ArrayAccess node result survives key destruction and is retired after the property store.
#[test]
fn test_simplexml_array_access_child_owners_bundle() {
    if skip_without_xml_native("test_simplexml_array_access_child_owners_bundle") { return; }
    let source = r#"<?php
class ArrayChildOwnerXmlCapability extends SimpleXMLElement {}
class ArrayChildOwnerNode { public mixed $value = 1; }
class ArrayChildOwnerKey { public function __destruct() { echo 'key|'; } }
class ArrayChildOwnerSource implements ArrayAccess {
    private ArrayChildOwnerNode $node;
    public function __construct() { $this->node = new ArrayChildOwnerNode(); }
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { return $this->node; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void {}
    public function snapshot(): mixed { return $this->node->value; }
}
function arrayChildOwnerProgram(): void {
    $source = new ArrayChildOwnerSource();
    $source[new ArrayChildOwnerKey()]->value = 2;
    echo $source->snapshot(), '|';
    unset($source);
}
arrayChildOwnerProgram();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "key|2|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
}
