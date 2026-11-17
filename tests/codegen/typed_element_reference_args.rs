//! Purpose:
//! Checks typed array elements passed to wider by-reference parameters: what the callee stores
//! is read back as itself, never decoded through the element's declared payload type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A typed local hash widens its entries to boxed Mixed cells before the element binds, like
//!   an indexed local already did, so the stored object or string reads back unchanged.
//! - A ref-bound typed local (a by-reference parameter or a `&` alias) shares its table with a
//!   frame that keeps the declared layout, so its element takes the typed write-back instead.
//! - An element of a typed instance or static property keeps its declared storage: it travels
//!   through a hidden boxed temporary, and the write-back applies the element type the way
//!   php-src applies a typed property's type to the reference it holds (weak coercion, or a
//!   `TypeError` naming the property).
//! - Every fixture runs under heap debugging and must leave a clean heap.

use crate::support::*;

/// Runs one fixture under heap debugging, requiring a clean heap, and returns its stdout.
fn run_clean(source: &str) -> String {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    output.stdout
}

/// A typed local hash element bound to `mixed &` reads back what the callee stored.
#[test]
fn test_typed_local_hash_element_reference_keeps_stored_value() {
    let source = r#"<?php
class Node { public function __construct(public string $name) {} }
function smash(mixed &$slot): void { $slot = new Node('n'); }
function seven(mixed &$slot): void { $slot = "7"; }
function run(): void
{
    array<int, int> $ids = [10, 20];
    smash($ids[0]);
    echo gettype($ids[0]), ":", $ids[0]->name, " ", $ids[1] + 1, "|";
    array<string, int> $counts = ["a" => 1];
    seven($counts["a"]);
    echo gettype($counts["a"]), ":", $counts["a"], "\n";
}
run();
"#;
    assert_eq!(run_clean(source), "object:n 21|string:7\n");
}

/// Instance and static property elements accept compatible writes with weak coercion and
/// reject an object with php-src's typed-reference `TypeError`, leaving the element intact.
#[test]
fn test_typed_property_element_reference_applies_element_type() {
    let source = r#"<?php
class Node {}
final class Holder
{
    public array<int, int> $items = [1, 2];
    public array<string, float> $weights = ["a" => 1.5];
    public static array<int, int> $shared = [5];
}
function seven(mixed &$slot): void { $slot = "7"; }
function smash(mixed &$slot): void { $slot = new Node(); }
function attempt(callable $call): void
{
    try { $call(); } catch (TypeError $error) { echo $error->getMessage(), "|"; }
}
$h = new Holder();
seven($h->items[0]);
seven($h->weights["a"]);
seven(Holder::$shared[0]);
echo gettype($h->items[0]), ":", $h->items[0] + 1, " ", gettype($h->weights["a"]), ":",
    $h->weights["a"], " ", Holder::$shared[0], "|";
attempt(function () use ($h): void { smash($h->items[1]); });
attempt(function (): void { smash(Holder::$shared[0]); });
echo $h->items[1], ",", Holder::$shared[0], "\n";
"#;
    assert_eq!(
        run_clean(source),
        "integer:8 double:7 7|Cannot assign Node to reference held by property Holder::$items \
element of type int|Cannot assign Node to reference held by property Holder::$shared element \
of type int|2,7\n"
    );
}

/// A by-reference typed parameter or an aliased typed local applies the element type through
/// the write-back, so the caller's table keeps its declared layout (Grok review scenario).
#[test]
fn test_ref_bound_typed_local_element_reference_applies_element_type() {
    let source = r#"<?php
class Node {}
function seven(mixed &$slot): void { $slot = "7"; }
function smash(mixed &$slot): void { $slot = new Node(); }
function mutate(array<string, int> &$counts): void
{
    seven($counts["a"]);
    echo gettype($counts["a"]), ":", $counts["a"], "|";
}
function indexed(array<int, int> &$ids): void
{
    try { smash($ids[0]); } catch (TypeError $error) { echo $error->getMessage(), "|"; }
}
function run(): void
{
    array<string, int> $counts = ["a" => 1, "b" => 2];
    mutate($counts);
    echo gettype($counts["a"]), ":", $counts["a"], " ", $counts["b"] + 1, "|";
    $alias = &$counts;
    seven($counts["b"]);
    echo $alias["b"] + 1, "|";
    array<int, int> $ids = [4, 5];
    indexed($ids);
    echo $ids[0], "\n";
}
run();
"#;
    assert_eq!(
        run_clean(source),
        "integer:7|integer:7 3|8|Cannot assign Node to reference held by $ids element of type \
int|4\n"
    );
}

/// A nested-array element of a typed property accepts an array of its own layout and keeps the
/// sibling untouched; a typed hash element accepts a hash with the declared value kind.
#[test]
fn test_typed_container_element_reference_converts_matching_values() {
    let source = r#"<?php
final class H
{
    public array<array<int>> $rows = [[4], [5]];
    public array<array<string, int>> $hashes = [["k" => 1]];
}
function pair(mixed &$slot): void { $slot = [7, 8]; }
function counts(mixed &$slot): void { $slot = ["k" => 9]; }
$h = new H();
pair($h->rows[0]);
counts($h->hashes[0]);
echo $h->rows[0][1] + 1, " ", $h->rows[1][0], " ", $h->hashes[0]["k"] + 1, "\n";
"#;
    assert_eq!(run_clean(source), "9 5 10\n");
}

/// A callee that leaves an object in a nested-array element stops the program with the typed
/// write-back diagnostic instead of corrupting the property's declared layout.
#[test]
fn test_typed_container_element_reference_rejects_foreign_value() {
    let source = r#"<?php
class Node {}
final class H { public array<array<int>> $rows = [[4], [5]]; }
function smash(mixed &$slot): void { $slot = new Node(); }
$h = new H();
smash($h->rows[0]);
echo "unreachable\n";
"#;
    let stderr = compile_and_run_expect_failure(source);
    assert!(stderr.contains("replaced by a value its typed variable cannot hold"), "{stderr}");
}

/// A hash element whose entries the callee filled with strings is rejected entry by entry: the
/// typed hash would otherwise read those string payloads as integers.
#[test]
fn test_typed_hash_element_reference_rejects_foreign_entries() {
    let source = r#"<?php
final class H { public array<array<string, int>> $hashes = [["k" => 1]]; }
function strings(mixed &$slot): void { $slot = ["k" => "s"]; }
$h = new H();
strings($h->hashes[0]);
echo "unreachable\n";
"#;
    let stderr = compile_and_run_expect_failure(source);
    assert!(stderr.contains("replaced by an array whose elements its typed variable cannot hold"),
        "{stderr}");
}

/// An indexed array of scalar hashes, as the element, is checked entry by entry inside each
/// nested hash: matching values are accepted, while foreign string entries stop the program.
#[test]
fn test_typed_nested_hash_element_reference_checks_inner_entries() {
    let accepted = r#"<?php
final class H { public array<array<array<string, int>>> $rows = [[["k" => 1]]]; }
function counts(mixed &$slot): void { $slot = [["k" => 9]]; }
$h = new H();
counts($h->rows[0]);
echo $h->rows[0][0]["k"] + 1, "\n";
"#;
    assert_eq!(run_clean(accepted), "10\n");
    let rejected = r#"<?php
final class H { public array<array<array<string, int>>> $rows = [[["k" => 1]]]; }
function strings(mixed &$slot): void { $slot = [["k" => "s"]]; }
$h = new H();
strings($h->rows[0]);
echo "unreachable\n";
"#;
    let stderr = compile_and_run_expect_failure(rejected);
    assert!(stderr.contains("replaced by an array whose elements its typed variable cannot hold"),
        "{stderr}");
}
