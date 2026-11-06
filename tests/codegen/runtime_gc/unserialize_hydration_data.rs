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

/// A later back-reference follows the replacement written under a duplicate key, like PHP.
#[test]
fn test_unserialize_completed_array_reference_survives_duplicate_key_replacement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
for ($i = 0; $i < 3; $i++) {
    $value = unserialize('a:3:{i:0;a:1:{s:4:"name";s:4:"kept";}i:0;i:5;i:1;R:2;}');
    echo $value[0], ":", $value[1], "|";
    unset($value);
}
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "5:5|5:5|5:5|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An object overwritten by a duplicate key leaves the later reference pointing at the
/// replacement; the replaced object is destroyed only after the parser completes.
#[test]
fn test_unserialize_object_reference_follows_duplicate_key_replacement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class DupObject { public function __destruct() { echo "drop|"; } }
for ($i = 0; $i < 3; $i++) {
    $value = unserialize('a:3:{i:0;O:9:"DupObject":0:{}i:0;i:5;i:1;R:2;}');
    echo $value[0], ":", $value[1], "|";
    unset($value);
}
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "drop|5:5|drop|5:5|drop|5:5|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The PHP 8.5.12 `var_destroy` shape: a destructor runs during duplicate-key cleanup and
/// recursively calls `unserialize('R:1500;')`. Elephc isolates the nested parse, so the
/// inner reference cannot reach the outer registry and the outer result stays intact.
#[test]
fn test_unserialize_destructor_reentry_cannot_read_the_outer_registry() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Reentrant {
    public function __destruct() {
        $inner = @unserialize('R:1500;');
        echo $inner === false ? "isolated|" : "outer-state|";
    }
}
$n = 2000;
$payload = 'a:' . ($n + 3) . ':{';
for ($i = 0; $i < $n; $i++) {
    $payload .= "i:$i;i:$i;";
}
$payload .= 'i:99999;O:9:"Reentrant":0:{}';
$payload .= 'i:99999;i:42;';
$payload .= 'i:' . $n . ';R:' . ($n + 2) . ';';
$payload .= '}';
$value = unserialize($payload);
echo $value[$n], "|done";
unset($value);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "isolated|42|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Scalar property slots copy the parsed payload inline; the parser's box must be
/// released after the store, and an unknown serialized key must not strand its value.
#[test]
fn test_unserialize_inline_scalar_properties_release_their_boxes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ScalarProps { public int $i = 0; public float $f = 0.0; public $inferred = 7; }
$value = unserialize('O:11:"ScalarProps":3:{s:1:"i";i:9;s:1:"f";d:1.5;s:8:"inferred";i:7;}');
echo $value->i, ":", $value->f, ":", $value->inferred, "|";
unset($value);
class EmptyProps { }
$other = unserialize('O:10:"EmptyProps":1:{s:1:"m";i:9;}');
echo get_class($other), "|done";
unset($other);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "9:1.5:7|EmptyProps|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Hydration overwrites each property's default value, so the default Mixed, string,
/// array, or object owner must be released before the parsed value replaces it. A
/// duplicate property key must release the value it overwrites as well.
#[test]
fn test_unserialize_overwritten_property_defaults_release_their_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class O { }
class DefaultProps {
    public mixed $m = null;
    public string $s = "";
    public array $a = [];
    public ?O $x = null;
}
$value = unserialize('O:12:"DefaultProps":4:{s:1:"m";i:9;s:1:"s";s:1:"x";s:1:"a";a:1:{i:0;i:1;}s:1:"x";O:1:"O":0:{}}');
echo $value->m, ":", $value->s, ":", count($value->a), ":", get_class($value->x), "|";
unset($value);
class DupProps { public mixed $m = null; }
$dup = unserialize('O:8:"DupProps":2:{s:1:"m";i:1;s:1:"m";i:2;}');
echo $dup->m, "|done";
unset($dup);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "9:x:1:O|2|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Overwriting a hydrated Mixed property must not free a box that an earlier
/// serialized value slot can still reach through `R:`.
#[test]
fn test_unserialize_overwritten_mixed_property_keeps_reference_slots_alive() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class RefA { }
class RefHolder { public mixed $m = null; public mixed $n = null; }
$value = unserialize('O:9:"RefHolder":3:{s:1:"m";O:4:"RefA":0:{}s:1:"m";i:5;s:1:"n";R:2;}');
echo $value->m, ":", $value->n, "|done";
unset($value, $keep);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "5:5|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The `__unserialize` data hash is built with the same duplicate-key rule, so a later
/// back-reference inside the property list follows the replacement value.
#[test]
fn test_unserialize_hook_data_reference_follows_duplicate_key_replacement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Plain {}
class Hydrated {
    public function __unserialize(array $data): void {
        echo $data["x"], ":", $data["y"], "|";
    }
}
$value = unserialize('O:8:"Hydrated":3:{s:1:"x";O:5:"Plain":0:{}s:1:"x";i:5;s:1:"y";R:2;}');
echo "done";
unset($value);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "5:5|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A duplicate key can replace an object with any other wire value: the old object's
/// destructor runs once, and both a scalar replacement and an object replacement stay
/// reachable from their published reference slots.
#[test]
fn test_unserialize_duplicate_key_replaces_object_with_scalar_and_object() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Source { public function __destruct() { echo "drop|"; } }
class Target { }
$scalar = unserialize('a:3:{i:0;O:6:"Source":0:{}i:0;s:3:"new";i:1;R:2;}');
echo $scalar[0], ":", $scalar[1], "|";
$object = unserialize('a:3:{i:0;O:6:"Source":0:{}i:0;O:6:"Target":0:{}i:1;r:2;}');
echo get_class($object[1]), "|done";
unset($scalar, $object);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "drop|new:new|drop|Target|done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The duplicate container's own reference slot follows the reused storage as well, so
/// both the replaced key and the duplicate occurrence observe the same array.
#[test]
fn test_unserialize_duplicate_container_reference_slot_stays_live() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$value = unserialize('a:3:{i:0;a:1:{s:1:"x";i:1;}i:0;a:1:{s:1:"x";i:9;}i:1;R:4;}');
echo $value[0]["x"], ":", $value[1]["x"], "|done";
unset($value);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "9:9|done", "{}", out.stderr);
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
