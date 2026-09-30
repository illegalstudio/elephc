//! Purpose:
//! Integration tests for `unset($obj->prop[$key])` on a property holding associative storage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - It had no direct lowering at all. The unset dispatch recognised a plain LOCAL receiver and
//!   nothing else, so a property receiver fell through to a synthetic `offsetUnset()` — which a
//!   class that does not implement `ArrayAccess` cannot answer — and the backend refused the call:
//!   `unset target shape with 1 lowered operands`. A compile error on ordinary php.
//! - The write has to LAND, which is the half that is easy to miss: `HashUnset` publishes a
//!   possibly-relocated table back through the receiver's place, and a `PropGet` reached through
//!   an `Acquire` is a place the backend already resolves.
//! - PACKED storage is deliberately still refused. Converting it at the unset site is what the
//!   local path does, but a property's type is one value for the WHOLE program rather than a
//!   flow-sensitive one, so recording that promotion retroactively rejects the `$obj->prop[] = $v`
//!   written above the unset. Left as it was rather than silently renumbering.

use crate::support::*;

/// Verifies removal, the count that follows it, and that other keys survive.
#[test]
fn unset_removes_one_key_from_an_array_property() {
    let out = compile_and_run_capture(
        r#"<?php
class Registry {
    public array $map = [];
    public function add(string $k, int $v): void { $this->map[$k] = $v; }
    public function drop(string $k): void { unset($this->map[$k]); }
}
$r = new Registry();
$r->add("a", 1);
$r->add("b", 2);
$r->add("c", 3);
$r->drop("b");
var_dump(count($r->map), array_keys($r->map));
unset($r->map["a"]);
var_dump(count($r->map), isset($r->map["a"]), isset($r->map["c"]));
unset($r->map["never there"]);
var_dump(count($r->map));
foreach ($r->map as $k => $v) { echo $k, "=", $v, ";"; }
echo "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "int(2)\n\
         array(2) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"c\"\n}\n\
         int(1)\n\
         bool(false)\n\
         bool(true)\n\
         int(1)\n\
         c=3;\n"
    );
}

/// Verifies the removal does not reach a copy taken beforehand, nor another instance.
///
/// The unset publishes a possibly-relocated table back into the property. If it wrote through a
/// shared pointer instead, a value copied out a line earlier would lose the key too.
#[test]
fn the_removal_reaches_only_that_objects_array() {
    let out = compile_and_run_capture(
        r#"<?php
class Registry {
    public array $map = [];
    public function add(string $k, int $v): void { $this->map[$k] = $v; }
}
$r = new Registry();
$r->add("c", 3);
$r->add("d", 4);
$copy = $r->map;
unset($r->map["c"]);
var_dump(count($copy), count($r->map));

$s = new Registry();
$s->add("x", 9);
unset($s->map["x"]);
var_dump(count($s->map), count($r->map));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "int(2)\nint(1)\nint(0)\nint(1)\n");
}
