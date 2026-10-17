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

/// A later array reference survives replacement of the earlier key that originally owned its box.
#[test]
fn test_unserialize_completed_array_reference_survives_duplicate_key_replacement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
for ($i = 0; $i < 3; $i++) {
    $value = unserialize('a:3:{i:0;a:1:{s:4:"name";s:4:"kept";}i:0;i:5;i:1;R:2;}');
    echo $value[0], ":", $value[1]["name"], "|";
    unset($value);
}
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "5:kept|5:kept|5:kept|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Publishing array targets must not allow lowercase object-identity references to read them.
#[test]
fn test_unserialize_completed_array_rejects_object_identity_reference() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$value = @unserialize('a:2:{i:0;a:0:{}i:1;r:2;}');
var_dump($value);
unset($value);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "bool(false)\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Untyped user hooks share the boxed signature for explicit method calls and decoder invocation.
#[test]
fn test_unserialize_untyped_hook_matches_direct_call_array_storage() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class U {
    public array $values = [];
    public function __unserialize($data): void { $this->values = $data; }
}
$direct = new U();
$direct->__unserialize(["x" => 7]);
$decoded = unserialize('O:1:"U":1:{s:1:"x";i:42;}');
echo $direct->values["x"], ":", $decoded->values["x"];
unset($direct, $decoded);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "7:42", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

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

/// A caught nested property error preserves outer identity and allowed-class policy.
#[test]
fn test_unserialize_exception_cleanup_nested_context() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class C { public int $n = 0; }
class D {}
class W { public function __wakeup(): void {
    try { unserialize('O:1:"C":1:{s:1:"n";s:1:"x";}'); }
    catch (TypeError $e) { echo "caught|"; }
} }
$options = ["allowed_classes" => ["W"]];
$value = unserialize('a:3:{i:0;O:1:"W":0:{}i:1;r:2;i:2;O:1:"D":0:{}}', $options);
echo ($value[0] === $value[1] ? "same" : "different"), "|", get_class($value[2]), "\n";
unset($value, $options);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught|same|__PHP_Incomplete_Class\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A typed-property throw releases every unfinished ancestor array and earlier child.
#[test]
fn test_unserialize_exception_cleanup_partial_parent() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class C { public int $n = 0; }
for ($i=0;$i<12;$i++) {
    try { unserialize('a:2:{i:0;s:4:"kept";i:1;O:1:"C":1:{s:1:"n";s:1:"x";}}'); }
    catch (TypeError $e) { echo "caught|"; }
}
echo unserialize('i:42;'), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught|caught|caught|caught|caught|caught|caught|caught|caught|caught|caught|caught|42\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A typed-property throw releases an unfinished hydration data hash before the hook runs.
#[test]
fn test_unserialize_exception_cleanup_partial_data() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class C { public int $n = 0; }
class H { public function __unserialize(array $data): void { echo "unexpected|"; } }
for ($i=0; $i<12; $i++) {
 try { unserialize('O:1:"H":2:{s:4:"kept";s:4:"text";s:3:"bad";O:1:"C":1:{s:1:"n";s:1:"x";}}'); }
 catch (TypeError $e) { echo "caught|"; }
}
echo unserialize('i:42;'), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught|caught|caught|caught|caught|caught|caught|caught|caught|caught|caught|caught|42\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A nested property failure releases its containing unfinished object and stored strings.
#[test]
fn test_unserialize_exception_cleanup_partial_object() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class C { public int $n = 0; }
class P { public mixed $text; public mixed $child; }
for ($i=0; $i<12; $i++) {
 try { unserialize('O:1:"P":2:{s:4:"text";s:4:"kept";s:5:"child";O:1:"C":1:{s:1:"n";s:1:"x";}}'); }
 catch (TypeError $e) { echo "caught|"; }
}
echo unserialize('i:42;'), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught|caught|caught|caught|caught|caught|caught|caught|caught|caught|caught|caught|42\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// User hook throws retire unfinished arrays and objects through the same cleanup chain.
#[test]
fn test_unserialize_exception_cleanup_hook_throw_partial() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class W { public function __wakeup(): void { throw new RuntimeException("wake"); } }
class H { public function __unserialize(array $data): void { throw new RuntimeException("hook"); } }
for ($i=0; $i<3; $i++) {
 try { unserialize('a:2:{i:0;s:4:"kept";i:1;O:1:"W":0:{}}'); }
 catch (RuntimeException $e) { echo $e->getMessage(), "|"; }
 try { unserialize('a:2:{i:0;s:4:"kept";i:1;O:1:"H":1:{s:4:"text";s:4:"data";}}'); }
 catch (RuntimeException $e) { echo $e->getMessage(), "|"; }
}
echo unserialize('i:42;'), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "wake|hook|wake|hook|wake|hook|42\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Option-validation errors still close their pre-boundary context exactly once.
#[test]
fn test_unserialize_exception_cleanup_nested_option_error() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class C { public int $n = 0; }
class D {}
class W { public function __wakeup(): void {
    $bad = ["allowed_classes" => 123];
    try { unserialize('i:1;', $bad); }
    catch (TypeError $e) { echo "option|"; }
    unset($bad);
    $bad = ["allowed_classes" => [123]];
    try { unserialize('i:1;', $bad); }
    catch (TypeError $e) { echo "element|"; }
    unset($bad);
} }
$options = ["allowed_classes" => ["W"]];
$value = unserialize('a:3:{i:0;O:1:"W":0:{}i:1;r:2;i:2;O:1:"D":0:{}}', $options);
echo ($value[0] === $value[1] ? "same" : "different"), "|", get_class($value[2]), "\n";
unset($value, $options);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "option|element|same|__PHP_Incomplete_Class\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Unwinding nested partial arrays releases earlier objects exactly once before the catch.
#[test]
fn test_unserialize_exception_cleanup_deep_parent() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class C { public int $n = 0; }
class D { public function __destruct() { echo "drop|"; } }
for ($i = 0; $i < 3; $i++) {
 try { unserialize('a:2:{i:0;O:1:"D":0:{}i:1;a:2:{i:0;s:4:"kept";i:1;O:1:"C":1:{s:1:"n";s:1:"x";}}}'); }
 catch (TypeError $e) { echo "caught|"; }
}
echo unserialize('i:42;'), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "drop|caught|drop|caught|drop|caught|42\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Throwing child destructors chain exceptions while all ancestor containers are retired.
#[test]
fn test_unserialize_exception_cleanup_cleanup_throw() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class C { public int $n = 0; }
class D { public function __destruct() { throw new RuntimeException("cleanup"); } }
for ($i = 0; $i < 3; $i++) {
 try { unserialize('a:2:{i:0;O:1:"D":0:{}i:1;O:1:"C":1:{s:1:"n";s:1:"x";}}'); }
 catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "|"; }
}
echo unserialize('i:42;'), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "RuntimeException:cleanup|RuntimeException:cleanup|RuntimeException:cleanup|42\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
