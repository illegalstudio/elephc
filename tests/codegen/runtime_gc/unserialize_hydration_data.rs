//! Purpose:
//! Verifies hydration hooks receive correctly boxed arrays with parser-scoped ownership.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Explicit wire fixtures isolate decoding from the independent serialization-magic path.
//! - Later back-references and nested decoders must keep discarded hook data alive until completion.

use crate::support::*;

/// A declared array hydration parameter can be read and retained as an ordinary PHP array.
#[test]
fn test_unserialize_declared_array_hydration_data_uses_boxed_storage() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H {
    public array $values = [];
    public function __unserialize(array $data): void { $this->values = $data; }
}
$value = unserialize('O:1:"H":2:{s:1:"x";i:42;s:4:"name";s:4:"kept";}');
echo $value->values["x"], ":", $value->values["name"], ":", implode(",", array_keys($value->values));
unset($value);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "42:kept:x,name", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Dropping the callback parameter must not invalidate a later reference to its decoded child.
#[test]
fn test_unserialize_discarded_hydration_data_keeps_later_back_references_alive() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H {
    public function __unserialize(array $data): void { echo $data["data"]["name"], "|"; }
}
$value = unserialize('a:2:{i:0;O:1:"H":1:{s:4:"data";a:1:{s:4:"name";s:4:"kept";}}i:1;R:3;}');
echo $value[1]["name"];
unset($value);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "kept|kept", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested decoding retires only its own data owners before the outer callback resumes reading.
#[test]
fn test_unserialize_nested_hydration_data_owners_are_isolated() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Inner {
    public function __unserialize(array $data): void { echo $data["text"], "|"; }
}
class Outer {
    public function __unserialize(array $data): void {
        $inner = unserialize('O:5:"Inner":1:{s:4:"text";s:5:"inner";}');
        echo $data["text"], "|";
        unset($inner);
    }
}
$value = unserialize('O:5:"Outer":1:{s:4:"text";s:5:"outer";}');
unset($value);
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "inner|outer|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A hydration-hook exception retires its context before a subsequent decode begins.
#[test]
fn test_unserialize_hydration_throw_restores_context_for_next_decode() {
    let source = r#"<?php
class H {
    public function __unserialize(array $data): void {
        if ($data["fail"]) { throw new RuntimeException("stop"); }
        echo "ok|";
    }
}
try { unserialize('O:1:"H":1:{s:4:"fail";b:1;}'); }
catch (Throwable $error) { echo $error->getMessage(), "|"; }
$value = unserialize('O:1:"H":1:{s:4:"fail";b:0;}');
echo unserialize('i:42;');
"#;
    assert_eq!(compile_and_run(source), "stop|ok|42");
}

/// Destructor failure while retiring discarded data propagates only after parser state is restored.
#[test]
fn test_unserialize_hydration_data_cleanup_throw_restores_context() {
    let source = r#"<?php
class G { public function __destruct() { throw new RuntimeException("cleanup"); } }
class H { public function __unserialize(array $data): void { echo "hook|"; } }
try { unserialize('O:1:"H":1:{s:4:"data";O:1:"G":0:{}}'); }
catch (Throwable $error) { echo $error->getMessage(), "|"; }
echo unserialize('i:42;');
"#;
    assert_eq!(compile_and_run(source), "hook|cleanup|42");
}
