//! Purpose:
//! Checks WeakMap value storage across cloning and entry references.
//!
//! Called from:
//! - The Core class codegen integration tests.
//!
//! Key details:
//! - Copies preserve object identity and detach array values through PHP COW.
//! - Managed references survive removal of their weak key and the map itself.

use crate::support::*;

/// Opaque eval keeps absent WeakMap offsets distinct from explicit null offsets.
#[test]
fn test_core_weak_map_denied_append_opaque_eval() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class EvalDeniedAppendHolder {
    public WeakMap $map;
    public static WeakMap $shared;
}
function evalDeniedAppendValue(): array { echo 'rhs;'; return ['v' => 1]; }
$source = '$map = new WeakMap();
$holder = new EvalDeniedAppendHolder();
$holder->map = $map;
EvalDeniedAppendHolder::$shared = $map;
try { $map[] = evalDeniedAppendValue(); }
catch (Error $e) { echo get_class($e), ":", $e->getMessage(), ";"; }
try { $holder->map[] = evalDeniedAppendValue(); }
catch (Error $e) { echo get_class($e), ":", $e->getMessage(), ";"; }
try { EvalDeniedAppendHolder::$shared[] = evalDeniedAppendValue(); }
catch (Error $e) { echo get_class($e), ":", $e->getMessage(), ";"; }
try { $map[null] = 1; }
catch (TypeError $e) { echo get_class($e), ";"; }
try { $map->offsetSet(null, 1); }
catch (TypeError $e) { echo get_class($e), ";"; }
echo count($map);
unset($holder, $map);';
eval($source . ' // ' . $argc);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "rhs;Error:Cannot append to WeakMap;rhs;Error:Cannot append to WeakMap;rhs;Error:Cannot append to WeakMap;TypeError;TypeError;0", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Missing keys evaluate their RHS and throw Error for every native receiver form.
#[test]
fn test_core_weak_map_denied_append() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class DeniedAppendHolder {
    public WeakMap $map;
    public static WeakMap $shared;
}
function deniedAppendValue(): array { echo 'rhs;'; return ['v' => 1]; }
$map = new WeakMap();
$holder = new DeniedAppendHolder();
$holder->map = $map;
DeniedAppendHolder::$shared = $map;
try { $map[] = deniedAppendValue(); }
catch (Error $e) { echo $e->getMessage(), ';'; }
try { $holder->map[] = deniedAppendValue(); }
catch (Error $e) { echo $e->getMessage(), ';'; }
try { DeniedAppendHolder::$shared[] = deniedAppendValue(); }
catch (Error $e) { echo $e->getMessage(), ';'; }
try { $map[null] = 1; }
catch (TypeError $e) { echo get_class($e), ';'; }
try { $map->offsetSet(null, 1); }
catch (TypeError $e) { echo get_class($e), ';'; }
echo count($map);
unset($holder, $map);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "rhs;Cannot append to WeakMap;rhs;Cannot append to WeakMap;rhs;Cannot append to WeakMap;TypeError;TypeError;0", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Entry increments reuse ordinary PHP string increment and postfix snapshot semantics.
#[test]
fn test_core_weak_map_string_increment_reference() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$key = new stdClass();
$map = new WeakMap();
$map[$key] = 'az';
$reference = &$map[$key];
$before = $map[$key]++;
echo $before, ':', $reference, ':', $map[$key];
unset($before, $reference, $map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "az:ba:ba", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Appending through an entry preserves a reference to its array value.
#[test]
fn test_core_weak_map_append_preserves_reference() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$key = new stdClass();
$map = new WeakMap();
$map[$key] = [1];
$reference = &$map[$key];
$map[$key][] = 2;
echo json_encode($reference), json_encode($map[$key]);
unset($reference, $map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "[1,2][1,2]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Entry argument leases outlive removal during a later argument's evaluation.
#[test]
fn test_core_weak_map_entry_argument_lease_survives_removal() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function replaceRemovedWeakEntry(mixed &$value, int $guard): void { $value = 9 + $guard; }
function removeWeakEntry(WeakMap $map, object $key): int { unset($map[$key]); return 0; }
$key = new stdClass();
$map = new WeakMap();
$map[$key] = ['n' => 0];
replaceRemovedWeakEntry($map[$key], removeWeakEntry($map, $key));
echo count($map);
unset($map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "0", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A closure's declared WeakMap result remains an addressable entry receiver.
#[test]
fn test_core_weak_map_closure_result_reference() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$map = new WeakMap();
$key = new stdClass();
$map[$key] = 1;
$get = fn(): WeakMap => $map;
$reference = &$get()[$key];
$reference = 2;
echo $map[$key], ':';
$maps = [$map];
$arrayReference = &$maps[0][$key];
$arrayReference = 3;
echo $map[$key], ':';
$expressionReference = &($get)()[$key];
$expressionReference = 4;
echo $map[$key], ':';
$literalReference = &(fn(): WeakMap => $map)()[$key];
$literalReference = 5;
echo $map[$key];
unset($reference, $arrayReference, $expressionReference, $literalReference, $maps, $get, $map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2:3:4:5", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Mutating builtins receive the same reference set as a direct user call.
#[test]
fn test_core_weak_map_mutating_builtin_reference() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$key = new stdClass();
$map = new WeakMap();
$map[$key] = 1;
$reference = &$map[$key];
settype($map[$key], 'string');
echo gettype($reference), ':', $map[$key];
unset($reference, $map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "string:1", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Prefix and postfix expression results snapshot the old/new value while retaining aliases.
#[test]
fn test_core_weak_map_incdec_expression_results() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$key = new stdClass();
$map = new WeakMap();
$map[$key] = 1;
$reference = &$map[$key];
$before = $map[$key]++;
$after = ++$map[$key];
echo $before, ':', $after, ':', $reference, ':', $map[$key];
unset($before, $after, $reference, $map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:3:3:3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested entry increments return their old/new snapshots while retaining the entry alias.
#[test]
fn test_core_weak_map_nested_incdec_expression_results() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function nestedWeakKey(object $key): object { echo 'key;'; return $key; }
$key = new stdClass();
$map = new WeakMap();
$map[$key] = 4;
$maps = [$map];
$reference = &$map[$key];
echo ++$maps[0][nestedWeakKey($key)], ':', $maps[0][$key], ':', $reference, ';';
echo $maps[0][nestedWeakKey($key)]--, ':', $maps[0][$key], ':', $reference;
$map[$key] = 'az';
$reference = &$map[$key];
echo ';', $maps[0][nestedWeakKey($key)]++, ':', $maps[0][$key], ':', $reference;
unset($reference, $maps, $map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "key;5:5:5;key;5:4:4;key;az:ba:ba", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested entry expression snapshots remain correct without EIR optimization.
#[test]
fn test_core_weak_map_nested_incdec_expression_results_no_ir_opt() {
    without_ir_opt(test_core_weak_map_nested_incdec_expression_results);
}

/// Direct reference parameters borrow the actual managed entry cell.
#[test]
fn test_core_weak_map_entry_by_ref_call() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function replaceWeakMapValue(mixed &$value): void { $value = 9; }
class ByRefWeakMapHolder {
    public WeakMap $map;
    public function __construct(WeakMap $map) { $this->map = $map; }
    public function take(): WeakMap { return $this->map; }
}
$key = new stdClass();
$map = new WeakMap();
$map[$key] = 1;
replaceWeakMapValue($map[$key]);
$holder = new ByRefWeakMapHolder($map);
replaceWeakMapValue($holder->map[$key]);
replaceWeakMapValue($holder->take()[$key]);
echo $map[$key];
unset($holder, $map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "9", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Increment preserves live references while compound and plain assignments detach them.
#[test]
fn test_core_weak_map_compound_reference_sets() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function compoundWeakKey(object $key): object { echo 'key;'; return $key; }
function compoundWeakValue(): int { echo 'right;'; return 3; }
$key = new stdClass();
$map = new WeakMap();
$map[$key] = 1;
$reference = &$map[$key];
$map[$key] += 1;
echo $reference, ':', $map[$key], ';';
$map[$key] = 1;
$reference = &$map[$key];
$map[compoundWeakKey($key)] += compoundWeakValue();
echo $reference, ':', $map[$key], ';';
$reference = &$map[$key];
$map[$key]++;
echo $reference, ':', $map[$key], ';';
$map[$key] = 7;
echo $reference, ':', $map[$key];
unset($reference, $map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:2;key;right;1:4;5:5;5:7", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing RHS preserves the value and retires ordinary write operands.
#[test]
fn test_core_weak_map_compound_throw_cleanup() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function failedWeakMapUpdate(): int { throw new Exception('rhs'); }
$key = new stdClass();
$map = new WeakMap();
$map[$key] = 1;
try { $map[$key] += failedWeakMapUpdate(); }
catch (Exception $error) { echo 'caught;'; }
echo $map[$key];
unset($map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught;1", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Static WeakMap properties expose the same managed entry storage as local receivers.
#[test]
fn test_core_weak_map_static_property_reference() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class StaticWeakMapHolder { public static WeakMap $map; }
StaticWeakMapHolder::$map = new WeakMap();
$key = new stdClass();
StaticWeakMapHolder::$map[$key] = 1;
$reference = &StaticWeakMapHolder::$map[$key];
$reference = 2;
StaticWeakMapHolder::$map[$key]++;
echo $reference, ':', StaticWeakMapHolder::$map[$key];
unset($reference, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "3:3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Property writes retire fresh keys and values when operand evaluation or offsetSet throws.
#[test]
fn test_core_weak_map_property_write_throwing_operands() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class WeakMapWriteHolder {
    public WeakMap $map;
    public function __construct() { $this->map = new WeakMap(); }
}
class WeakMapWriteKey {
    public function __destruct() { echo 'key;'; }
}
function failingWeakMapValue(): mixed { throw new Exception('value'); }
function freshWeakMapValue(): array { return ['n' => 1]; }
$holder = new WeakMapWriteHolder();
try { $holder->map[new WeakMapWriteKey()] = failingWeakMapValue(); }
catch (Exception $error) { echo 'value;'; }
try { $holder->map['invalid'] = freshWeakMapValue(); }
catch (TypeError $error) { echo 'type;'; }
echo count($holder->map);
unset($holder);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "key;value;type;0", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Property and method receivers bind the same entry and evaluate each operand once.
#[test]
fn test_core_weak_map_reference_receiver_expressions() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class WeakMapHolder {
    public WeakMap $map;
    public int $calls = 0;
    public function __construct() { $this->map = new WeakMap(); }
    public function take(): WeakMap { $this->calls++; return $this->map; }
    public static function takeStatic(WeakMapHolder $holder): WeakMap {
        $holder->calls++;
        return $holder->map;
    }
}
function keyOnce(WeakMapHolder $holder, object $key): object {
    $holder->calls++;
    return $key;
}
$holder = new WeakMapHolder();
$key = new stdClass();
$holder->map[$key] = ['n' => 0];
$propertyReference = &$holder->map[$key];
$propertyReference['n'] = 1;
$methodReference = &$holder->take()[keyOnce($holder, $key)];
$methodReference['n'] = 2;
$staticReference = &WeakMapHolder::takeStatic($holder)[$key];
$staticReference['n'] = 3;
echo $propertyReference['n'], ':', $methodReference['n'], ':', $holder->map[$key]['n'], ':', $holder->calls;
unset($holder, $key);
echo ';', $staticReference['n'];
unset($propertyReference, $methodReference, $staticReference);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "3:3:3:3;3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque eval clones the native map while an AOT mutation keeps the clone's array independent.
#[test]
fn test_core_weak_map_clone_cow_across_opaque_eval() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function changeNativeWeakMapValue(WeakMap $map, object $key): void { $map[$key]['n'] = 1; }
$source = '$key = new stdClass();
$map = new WeakMap();
$map[$key] = ["n" => 0];
$copy = clone $map;
changeNativeWeakMapValue($map, $key);
echo $map[$key]["n"], ":", $copy[$key]["n"];
unset($copy, $map, $key);';
eval($source . ' // ' . $argc);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:0", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Map cloning separates nested array mutations even across repeated clones.
#[test]
fn test_core_weak_map_clone_separates_nested_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$key = new stdClass();
$map = new WeakMap();
$map[$key] = ['n' => 0];
$value = $map[$key];
$copy = clone $map;
$map[$key]['n'] = 1;
$copy[$key]['n'] = 2;
echo $map[$key]['n'], ':', $value['n'], ':', $copy[$key]['n'], ';';
$again = clone $copy;
$again[$key]['n'] = 3;
echo $copy[$key]['n'], ':', $again[$key]['n'];
unset($value, $again, $copy, $map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:0:2;2:3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Replacing an entry detaches its old reference, which outlives the key and map.
#[test]
fn test_core_weak_map_reference_lifetime_and_replacement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$key = new stdClass();
$map = new WeakMap();
$map[$key] = ['n' => 0];
$reference = &$map[$key];
$reference['n'] = 2;
echo $map[$key]['n'], ':';
$map[$key] = ['n' => 3];
echo $reference['n'], ':', $map[$key]['n'], ':';
unset($key);
echo count($map), ':', $reference['n'], ':';
unset($map);
$reference['n'] = 4;
echo $reference['n'];
unset($reference);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2:2:3:0:2:4", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A clone preserves live reference sets and separates entries whose reference is orphaned.
#[test]
fn test_core_weak_map_clone_reference_sets() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$key = new stdClass();
$map = new WeakMap();
$map[$key] = 0;
$reference = &$map[$key];
$copy = clone $map;
$reference = 2;
echo $map[$key], ':', $copy[$key], ';';
unset($reference, $copy);
$orphanCopy = clone $map;
$orphanCopy[$key] = 3;
echo $map[$key], ':', $orphanCopy[$key];
unset($orphanCopy, $map, $key);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2:2;2:3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
