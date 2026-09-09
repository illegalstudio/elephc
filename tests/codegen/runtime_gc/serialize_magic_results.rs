//! Purpose:
//! Checks native magic serializer return layouts and ownership across recursive encoding.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Declared array returns cross a boxed Mixed boundary, unlike concrete inferred arrays.
//! - Both execution modes check wire bytes; heap debugging also checks exceptional cleanup.

use crate::support::*;

/// Indexed and associative magic results retain their payloads until their wire bodies are complete.
#[test]
fn test_core_serialize_boxed_magic_array_results_release_their_owners() {
    let source = r#"<?php
class P {
    public function __serialize(): array { return ['x' => 1, 'y' => str_repeat('a', 3)]; }
}
class Q {
    public function __serialize(): array { return [5, str_repeat('h', 2)]; }
}
for ($i = 0; $i < 2; $i++) {
    $p = new P();
    $q = new Q();
    echo 'prefix|' . serialize($p), '|', serialize($q), "\n";
    unset($p, $q);
}
"#;
    let expected = "prefix|O:1:\"P\":2:{s:1:\"x\";i:1;s:1:\"y\";s:3:\"aaa\";}|O:1:\"Q\":2:{i:0;i:5;i:1;s:2:\"hh\";}\n";
    assert_clean_magic_result(source, &expected.repeat(2));
}

/// Concrete inferred return storage remains valid alongside declared boxed array returns.
#[test]
fn test_core_serialize_inferred_magic_array_result_remains_supported() {
    let source = r#"<?php
class U {
    public function __serialize() { return ['z' => str_repeat('b', 2)]; }
}
$u = new U();
echo serialize($u);
unset($u);
"#;
    assert_clean_magic_result(source, "O:1:\"U\":1:{s:1:\"z\";s:2:\"bb\";}");
}

/// A dynamic non-array return is rejected before reading a container header and is still released.
#[test]
fn test_core_serialize_invalid_magic_result_is_catchable_and_released() {
    let source = r#"<?php
function invalidMagicResult(int $count): mixed { return str_repeat('bad', $count); }
class Bad {
    public function __serialize() { return invalidMagicResult(2); }
}
$bad = new Bad();
try { serialize($bad); }
catch (TypeError $error) { echo get_class($error), '|'; unset($error); }
unset($bad);
echo serialize(9);
"#;
    assert_clean_magic_result(source, "TypeError|i:9;");
}

/// Releasing the magic return owner must not invalidate a separately owned object property.
#[test]
fn test_core_serialize_magic_shared_property_array_keeps_its_owner() {
    let source = r#"<?php
class S {
    public array $values = ['k' => 'v'];
    public function __serialize(): array { return $this->values; }
}
$s = new S();
echo serialize($s), '|', serialize($s), '|', $s->values['k'];
unset($s);
"#;
    assert_clean_magic_result(source, "O:1:\"S\":1:{s:1:\"k\";s:1:\"v\";}|O:1:\"S\":1:{s:1:\"k\";s:1:\"v\";}|v");
}

/// An exception from a nested magic hook still retires its enclosing temporary array and children.
#[test]
fn test_core_serialize_magic_nested_throw_releases_returned_array() {
    let source = r#"<?php
class I {
    public function __serialize(): array { throw new Exception('stop'); }
    public function __destruct() { echo 'inner|'; }
}
class O {
    public function __serialize(): array { return ['text' => str_repeat('x', 4), 'child' => new I()]; }
}
for ($i = 0; $i < 2; $i++) {
    $o = new O();
    try { serialize($o); }
    catch (Exception $error) { echo $error->getMessage(), '|'; unset($error); }
    unset($o);
}
echo serialize(7);
"#;
    assert_clean_magic_result(source, "inner|stop|inner|stop|i:7;");
}

/// A destructor throwing while the return array is retired propagates only after that array is freed.
#[test]
fn test_core_serialize_magic_result_destructor_throw_retires_container() {
    let source = r#"<?php
class V {
    public function __destruct() { echo 'drop|'; throw new Exception('cleanup'); }
}
class R {
    public function __serialize(): array { return [new V()]; }
}
$r = new R();
try { serialize($r); }
catch (Exception $error) { echo $error->getMessage(), '|'; unset($error); }
unset($r);
echo serialize(8);
"#;
    assert_clean_magic_result(source, "drop|cleanup|i:8;");
}

/// Requires exact serialization output and balanced native heap ownership without weakening failures.
fn assert_clean_magic_result(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}
