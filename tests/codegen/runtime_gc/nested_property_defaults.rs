//! Purpose:
//! Heap-debug coverage for property defaults whose elements are themselves array literals.
//! Each nested container is allocated during the object's initialization and handed to the
//! container enclosing it, so the whole tree has to be owned by its root and released exactly
//! once with the object.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`. The two ways to
//!   get the transfer wrong land on opposite sides of that assertion: retaining the child
//!   without releasing the builder's reference leaks the whole subtree once per object, and
//!   releasing it without the retain frees a child the object still points at, which shows up
//!   as corrupted reads or a double free rather than as a leak -- so the fixtures read the
//!   values back as well.
//! - The loops allocate hundreds of objects, so a per-object leak cannot hide in heap slack.
//! - Expected stdout values are real `LC_ALL=C php` 8.5 output for the same fixtures.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// An indexed default holding indexed literals releases its whole tree with the object.
#[test]
fn test_indexed_nested_property_default_releases_with_the_object() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class A { public array $x = [[1], [2]]; }
$t = 0;
for ($i = 0; $i < 300; $i++) {
    $a = new A();
    $t += count($a->x) + $a->x[1][0];
    unset($a);
}
echo $t;
"#,
    );
    assert_clean(out, "1200");
}

/// The keyed and boxed spellings release too, including a tree that mixes both.
///
/// The hash path transfers ownership differently from the indexed one -- `__rt_hash_set` takes
/// the value it stores rather than retaining it, where the indexed path boxes into a Mixed cell
/// and releases the builder's reference -- so a single fixture cannot cover both.
#[test]
fn test_keyed_and_boxed_nested_property_defaults_release_with_the_object() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class B { public mixed $x = ["a" => [1, 2], "b" => ["c" => "d"], "e" => 3]; }
class C { public array $x = ["k" => ["j" => 7]]; }
$t = 0;
for ($i = 0; $i < 300; $i++) {
    $b = new B();
    $t += count($b->x) + $b->x["a"][1];
    unset($b);
    $c = new C();
    $t += $c->x["k"]["j"];
    unset($c);
}
echo $t;
"#,
    );
    assert_clean(out, "3600");
}

/// A deep tree with string, float and null leaves stays balanced.
///
/// Strings are the leaf that can go wrong independently: they are persisted rather than
/// refcounted like a container, so a tree carrying both has to get two ownership rules right
/// at once.
#[test]
fn test_deep_nested_property_default_with_mixed_leaves_releases_cleanly() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class D { public array $x = [[[1, "s", 2.5, null, true]]]; }
$t = 0;
for ($i = 0; $i < 300; $i++) {
    $d = new D();
    $t += count($d->x[0][0]);
    unset($d);
}
echo $t;
"#,
    );
    assert_clean(out, "1500");
}

/// A copy taken out of the default outlives the object it came from.
///
/// This is the over-release side of the contract: if the object's release freed a child the
/// copy still holds, reading the copy afterwards reads freed memory. The loop repeats it so a
/// recycled block would come back with someone else's contents.
#[test]
fn test_copy_of_a_nested_default_survives_its_object() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class A { public array $x = [[1], [2]]; }
$t = 0;
for ($i = 0; $i < 300; $i++) {
    $a = new A();
    $inner = $a->x[0];
    unset($a);
    $t += $inner[0] + count($inner);
    unset($inner);
}
echo $t;
"#,
    );
    assert_clean(out, "600");
}
