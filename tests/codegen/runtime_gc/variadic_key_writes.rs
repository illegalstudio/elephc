//! Purpose:
//! Checks string-key writes and nested appends on lists and reference variadic copies.
//!
//! Called from:
//! - The runtime GC codegen integration test module.
//!
//! Key details:
//! - Reference markers survive container copies and promotion without becoming scalar values.
//! - Missing nested parents are installed in the mutated container, with balanced ownership.

use crate::support::compile_and_run_with_heap_debug;

/// Requires PHP-visible values, silent legal autovivification and a clean final heap.
fn assert_key_write(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(!out.stderr.contains("Undefined array key"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Computed string and boxed keys install nested parents in typed lists without changing aliases.
#[test]
fn test_variadic_fourth_review_typed_list_computed_parent() {
    assert_key_write(r#"<?php
function writeString(string $key): void {
    $items = ["A", "B", "C"]; $copy = $items;
    $items[$key]["x"] = 1; $items[$key]["y"] = 2;
    echo json_encode($items), "|", json_encode($copy), "\n";
}
function writeMixed(mixed $key): void {
    $items = [5, 7]; $copy = $items;
    $items[$key]["x"] = 3;
    echo json_encode($items), "|", json_encode($copy), "\n";
}
writeString("key"); writeMixed("key");
"#, "{\"0\":\"A\",\"1\":\"B\",\"2\":\"C\",\"key\":{\"x\":1,\"y\":2}}|[\"A\",\"B\",\"C\"]\n{\"0\":5,\"1\":7,\"key\":{\"x\":3}}|[5,7]\n");
}

/// Numeric-string keys select existing strings and keep failed writes local to the mutation.
#[test]
fn test_variadic_fifth_review_numeric_string_parent() {
    assert_key_write(r#"<?php
function writeString(string $key): void {
    $items = ["A", "B", "C"]; $copy = $items;
    try { $items[$key]["x"] = 1; echo 'wrong'; }
    catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(); }
    echo '|', json_encode($items), '|', json_encode($copy);
}
writeString("1");
"#, "TypeError:Cannot access offset of type string on string|[\"A\",\"B\",\"C\"]|[\"A\",\"B\",\"C\"]");
}

/// A scalar integer parent raises the normal PHP Error without altering its COW sibling.
#[test]
fn test_variadic_fifth_review_numeric_integer_parent() {
    assert_key_write(r#"<?php
function writeInt(string $key): void {
    $items = [10, 20, 30]; $copy = $items;
    try { $items[$key]["x"] = 1; echo 'wrong'; }
    catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(); }
    echo '|', json_encode($items), '|', json_encode($copy);
}
writeInt("1");
"#, "Error:Cannot use a scalar value as an array|[10,20,30]|[10,20,30]");
}

/// Branching keys and same-local replacement retire each Mixed source slot exactly once.
#[test]
fn test_variadic_fifth_review_branching_mixed_source_slot_cleanup() {
    assert_key_write(r#"<?php
function branchingKey(int $condition): void {
    $items = [10, 20, 30]; $copy = $items;
    try { $items[$condition ? "1" : "0"]["x"] = 1; echo 'wrong'; }
    catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(); }
    echo '|', json_encode($items), '|', json_encode($copy), "\n";
}
function republishedKey(int $condition): void {
    $items = [[0], [0]];
    $items["seed"] = [0]; $copy = $items;
    $items[($items = [[0], [0]]) ? ($condition ? 1 : 0) : 0][0] = 1;
    echo json_encode($items), '|', json_encode($copy), "\n";
}
branchingKey($argc);
republishedKey($argc);
"#, concat!(
        "Error:Cannot use a scalar value as an array|[10,20,30]|[10,20,30]\n",
        "[[0],[1]]|{\"0\":[0],\"1\":[0],\"seed\":[0]}\n",
    ));
}

/// Source-slot retirement remains balanced without optional EIR optimization.
#[test]
fn test_variadic_fifth_review_branching_mixed_source_slot_cleanup_without_ir_opt() {
    crate::support::without_ir_opt(
        test_variadic_fifth_review_branching_mixed_source_slot_cleanup,
    );
}

/// Appending to a selected string raises a catchable Error, not a codegen diagnostic.
#[test]
fn test_variadic_fifth_review_numeric_string_append() {
    assert_key_write(r#"<?php
function appendString(string $key): void {
    $items = ["A", "B", "C"]; $copy = $items;
    try { $items[$key][] = 1; echo 'wrong'; }
    catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(); }
    echo '|', json_encode($items), '|', json_encode($copy);
}
appendString("1");
"#, "Error:[] operator not supported for strings|[\"A\",\"B\",\"C\"]|[\"A\",\"B\",\"C\"]");
}

/// Computed parent keys and RHS effects run once before a selected string's offset error.
#[test]
fn test_variadic_fifth_review_owned_numeric_string_order() {
    assert_key_write(r#"<?php
function pickParentKey(): string { echo 'K'; return '1'; }
function value(): int { echo 'V'; return 1; }
$items = ["A", "B", "C"]; $copy = $items;
try { $items[pickParentKey()]["x"] = value(); echo 'wrong'; }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(); }
echo '|', json_encode($items), '|', json_encode($copy);
"#, "KVTypeError:Cannot access offset of type string on string|[\"A\",\"B\",\"C\"]|[\"A\",\"B\",\"C\"]");
}

/// Missing numeric and named parents still autovivify after a failed scalar append.
#[test]
fn test_variadic_fifth_review_missing_parent_controls() {
    assert_key_write(r#"<?php
function appendValue(): int { echo 'V'; return 1; }
function writeParents(string $existing, string $missing, string $named): void {
    $items = ['A', 'B', 'C']; $copy = $items;
    try { $items[$existing][] = appendValue(); }
    catch (Error $error) { echo $error->getMessage(), '|'; }
    $items[$missing]['x'] = 2;
    $items[$named][] = 3; $items[$named][] = 4;
    echo json_encode($items), '|', json_encode($copy);
}
writeParents('1', '3', 'key');
"#, "V[] operator not supported for strings|{\"0\":\"A\",\"1\":\"B\",\"2\":\"C\",\"3\":{\"x\":2},\"key\":[3,4]}|[\"A\",\"B\",\"C\"]");
}

/// A concrete string-list bucket must not reach an unsupported codegen append.
#[test]
fn test_variadic_fifth_review_concrete_string_append() {
    assert_key_write(r#"<?php
function appendString(string $key): void {
    $items = ['A', 'B'];
    try { $items[$key][] = 1; }
    catch (Error $error) { echo $error->getMessage(), '|'; }
    echo json_encode($items);
}
appendString('1');
"#, "[] operator not supported for strings|[\"A\",\"B\"]");
}

/// Failed nested writes retire owning RHS values and parent cells before entering the catch.
#[test]
fn test_variadic_fifth_review_owned_rhs_cleanup() {
    assert_key_write(r#"<?php
function ownedValue(): string { return str_repeat('x', 40); }
function throwingValue(): int { throw new Exception('rhs'); }
function writeOwned(string $key): void {
    $items = ['A', 'B']; $copy = $items;
    try { $items[$key]['x'] = ownedValue(); }
    catch (TypeError $error) { echo 'offset|'; }
    try { $items[$key][] = ownedValue(); }
    catch (Error $error) { echo 'append|'; }
    try { $items[$key]['x'] = throwingValue(); }
    catch (Exception $error) { echo $error->getMessage(), '|'; }
    echo json_encode($items), '|', json_encode($copy);
}
writeOwned('1');
"#, "offset|append|rhs|[\"A\",\"B\"]|[\"A\",\"B\"]");
}

/// An owning string call is evaluated once per parent fetch on a typed string list.
#[test]
fn test_variadic_fourth_review_typed_list_owned_parent() {
    assert_key_write(r#"<?php
function makeKey(string $suffix): string { echo "K"; return "k" . $suffix; }
function writeOwned(string $suffix): void {
    $items = ["A", "B"]; $copy = $items;
    $items[makeKey($suffix)]["x"] = 1;
    $items[makeKey($suffix)]["y"] = 2;
    echo json_encode($items), "|", json_encode($copy);
}
writeOwned("ey");
"#, "KK{\"0\":\"A\",\"1\":\"B\",\"key\":{\"x\":1,\"y\":2}}|[\"A\",\"B\"]");
}

/// A by-value variadic copy retains referenced caller slots and reads their current values.
#[test]
fn test_variadic_third_review_copy_numeric_string_key() {
    assert_key_write(r#"<?php
function writeCopy(&...$items): void {
    $copy = $items;
    $copy["1"] = "Q";
    echo $copy[0], "/", $copy[1], "/", $copy[2], "|", json_encode($copy), "|";
}
$a = "A"; $b = "B"; $c = "C";
$write = writeCopy(...); $write($a, $b, $c);
echo $a, "/", $b, "/", $c, "\n";
"#, "A/Q/C|[\"A\",\"Q\",\"C\"]|A/Q/C\n");
}

/// Creating a named nested parent does not overwrite an unrelated variadic caller slot.
#[test]
fn test_variadic_third_review_nested_string_parent() {
    assert_key_write(r#"<?php
function writeNested(&...$items): void {
    $items["key"]["x"] = 1;
    echo $items[0], "/", $items[1], "/", $items[2], "|", json_encode($items), "|";
}
$a = "A"; $b = "B"; $c = "C";
$write = writeNested(...); $write($a, $b, $c);
echo $a, "/", $b, "/", $c, "\n";
"#, "A/B/C|{\"0\":\"A\",\"1\":\"B\",\"2\":\"C\",\"key\":{\"x\":1}}|A/B/C\n");
}

/// The same nested parent is attached to a plain list without changing its COW sibling.
#[test]
fn test_variadic_third_review_plain_list_nested_parent() {
    assert_key_write(r#"<?php
$items = ["A", "B", "C"]; $copy = $items;
$items["key"]["x"] = 1;
echo json_encode($items), "|", json_encode($copy), "\n";
"#, "{\"0\":\"A\",\"1\":\"B\",\"2\":\"C\",\"key\":{\"x\":1}}|[\"A\",\"B\",\"C\"]\n");
}

/// A string-key nested append can probe and grow a reference variadic container.
#[test]
fn test_variadic_third_review_nested_string_append() {
    assert_key_write(r#"<?php
function appendNested(&...$items): void {
    $items["key"][] = 1; $items["key"][] = 2;
    echo $items[0], "/", $items[1], "/", $items[2], "|", json_encode($items), "|";
}
$a = "A"; $b = "B"; $c = "C";
$write = appendNested(...); $write($a, $b, $c);
echo $a, "/", $b, "/", $c, "\n";
"#, "A/B/C|{\"0\":\"A\",\"1\":\"B\",\"2\":\"C\",\"key\":[1,2]}|A/B/C\n");
}

/// Repeated nested appends on a plain list preserve both prior values and its copied sibling.
#[test]
fn test_variadic_third_review_plain_list_nested_append() {
    assert_key_write(r#"<?php
$items = ["A", "B", "C"]; $copy = $items;
$items["key"][] = 1; $items["key"][] = 2;
echo json_encode($items), "|", json_encode($copy), "\n";
"#, "{\"0\":\"A\",\"1\":\"B\",\"2\":\"C\",\"key\":[1,2]}|[\"A\",\"B\",\"C\"]\n");
}

/// A nested write into a by-value variadic copy adds only its private named parent.
#[test]
fn test_variadic_third_review_copy_nested_string_parent() {
    assert_key_write(r#"<?php
function writeCopyNested(&...$items): void {
    $copy = $items; $copy["key"]["x"] = 1;
    echo json_encode($copy), "|", json_encode($items), "|";
}
$a = "A"; $b = "B"; $c = "C";
$write = writeCopyNested(...); $write($a, $b, $c);
echo $a, "/", $b, "/", $c, "\n";
"#, "{\"0\":\"A\",\"1\":\"B\",\"2\":\"C\",\"key\":{\"x\":1}}|[\"A\",\"B\",\"C\"]|A/B/C\n");
}

/// Runtime string-key isset probes stay silent, recognize null and retire their result cells.
#[test]
fn test_variadic_third_review_mixed_key_isset_control() {
    assert_key_write(r#"<?php
function probeKeys(array $items, mixed $key): void {
    echo isset($items[$key]) ? "yes" : "no", ":";
    echo isset($items["missing"]) ? "yes" : "no", "\n";
}
$items = [null, "B"]; $key = $argc == 1 ? "1" : 1;
probeKeys($items, $key); probeKeys($items, "0");
$items["key"] = "V"; probeKeys($items, "key");
$items["key"] = null; probeKeys($items, "key");
"#, "yes:no\nno:no\nyes:no\nno:no\n");
}

/// An owned computed string parent is used once and freed after repeated variadic writes.
#[test]
fn test_variadic_third_review_owned_string_parent() {
    assert_key_write(r#"<?php
function parentKey(string $suffix): string { return "k" . $suffix; }
function writeOwnedKeys(string $suffix, &...$items): void {
    $items[parentKey($suffix)]["x"] = 1;
    $items[parentKey($suffix)]["y"] = 2;
    echo json_encode($items), "|";
}
$a = "A"; $b = "B"; $c = "C";
$write = writeOwnedKeys(...); $write("ey", $a, $b, $c);
echo $a, "/", $b, "/", $c, "\n";
"#, "{\"0\":\"A\",\"1\":\"B\",\"2\":\"C\",\"key\":{\"x\":1,\"y\":2}}|A/B/C\n");
}
