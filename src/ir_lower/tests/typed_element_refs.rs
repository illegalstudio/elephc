//! Purpose:
//! Verifies that typed array elements the by-reference write-back cannot validate are refused
//! when they are bound to a wider by-reference parameter.
//!
//! Called from:
//! - The AST-to-EIR unit test suite.
//!
//! Key details:
//! - A hash element whose values are objects or arrays would need a per-entry class or layout
//!   check, and a pointer element cannot be told apart once boxed, so the lowering refuses both
//!   with a source diagnostic on every target, for property and ref-bound local containers.

use crate::codegen::platform::Target;
use std::path::Path;

/// Asserts that lowering `source` is refused with the typed-element diagnostic on every target.
fn assert_refused(source: &str) {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let error = super::try_lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        ).expect_err("an object hash element cannot be validated on write-back");
        let crate::ir_lower::LoweringError::Unsupported(error) = error else {
            panic!("{name}: unexpected lowering error {error:?}");
        };
        assert!(error.message.contains("cannot be passed to a wider by-reference parameter"),
            "{name}: {}", error.message);
    }
}

/// A typed hash element of objects passed to `mixed &` is refused with a clear diagnostic.
#[test]
fn typed_object_hash_element_reference_is_refused_on_every_target() {
    assert_refused(r#"<?php
class Node {}
final class H { public array<array<string, Node>> $hashes = []; }
function smash(mixed &$slot): void { $slot = new Node(); }
$h = new H();
smash($h->hashes[0]);
"#);
}

/// Pointer elements of a property or of a ref-bound local, indexed or keyed, are refused
/// before binding could box every pointer word of the shared container.
#[test]
fn typed_pointer_element_reference_is_refused_on_every_target() {
    let prelude = r#"<?php
packed class Vec2 { public int $x; public int $y; }
function smash(mixed &$slot): void { $slot = 1; }
"#;
    for body in [
        "final class Bag { public array<ptr<Vec2>> $items; }\n$b = new Bag();\nsmash($b->items[0]);\n",
        "final class Bag { public array<string, ptr<Vec2>> $items; }\n$b = new Bag();\nsmash($b->items[\"k\"]);\n",
        "function outer(array<ptr<Vec2>> &$items): void { smash($items[0]); }\n",
        "function outer(array<string, ptr<Vec2>> &$items): void { smash($items[\"k\"]); }\n",
    ] {
        assert_refused(&format!("{prelude}{body}"));
    }
}

/// A hash of objects wrapped in an indexed array element is refused too, for a property and
/// for a ref-bound local, because the nested hash's entries cannot be class-checked.
#[test]
fn typed_nested_object_hash_element_reference_is_refused_on_every_target() {
    let prelude = r#"<?php
class Node {}
function smash(mixed &$slot): void { $slot = [["k" => "s"]]; }
"#;
    for body in [
        "final class H { public array<array<array<string, Node>>> $rows = []; }\n$h = new H();\nsmash($h->rows[0]);\n",
        "function outer(array<array<array<string, Node>>> &$rows): void { smash($rows[0]); }\n",
    ] {
        assert_refused(&format!("{prelude}{body}"));
    }
}

/// A plain local hash of pointers is refused instead of being widened, which would box every
/// pointer word of the hash as an integer before the callee runs.
#[test]
fn plain_local_pointer_hash_element_reference_is_refused_on_every_target() {
    assert_refused(r#"<?php
packed class Vec2 { public int $x; public int $y; }
function smash(mixed &$slot): void {}
function run(ptr<Vec2> $p): void {
    array<string, ptr<Vec2>> $items = ["k" => $p, "other" => $p];
    smash($items["k"]);
}
"#);
}
