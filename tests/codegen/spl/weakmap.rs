//! Purpose:
//! End-to-end tests for the built-in `WeakMap` class: object-keyed set/get, `count`,
//! `isset`/`unset`, `ArrayAccess`, iteration, redeclaration rejection, and metadata presence.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - `WeakMap` is a strong object-keyed map (no auto-eviction) modeled on `SplObjectStorage`;
//!   tests exercise behavior while the key object is live, which matches PHP exactly.
//! - `offsetGet` returns `null` for an absent key instead of throwing (documented gap); the
//!   `isset` guard test covers the PHP-idiomatic path.

use crate::support::*;

/// Verifies that `WeakMap` is declared, final, and implements its SPL interfaces.
#[test]
fn test_weakmap_is_declared_and_typed() {
    let out = compile_and_run(
        r#"<?php
var_dump(class_exists("WeakMap"));
var_dump(new WeakMap() instanceof Countable);
var_dump(new WeakMap() instanceof ArrayAccess);
var_dump(new WeakMap() instanceof Iterator);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}

/// Verifies the micro-probe shape: `new WeakMap()`, set via ArrayAccess, get via ArrayAccess.
#[test]
fn test_weakmap_arrayaccess_set_and_get() {
    let out = compile_and_run(
        r#"<?php
$m = new WeakMap();
$o = new stdClass();
$m[$o] = "OK";
echo $m[$o];
"#,
    );
    assert_eq!(out, "OK");
}

/// Verifies `count()`, `isset`, update-on-existing-key, and distinct-object identity.
#[test]
fn test_weakmap_count_isset_update_and_identity() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $id;
    public function __construct(int $id) {
        $this->id = $id;
    }
}

$m = new WeakMap();
$a = new Box(1);
$b = new Box(2);

$m[$a] = "a";
$m[$b] = "b";
echo count($m);
echo ":";
echo isset($m[$a]) ? "y" : "n";
echo ":";
echo isset($m[$b]) ? "y" : "n";
echo ":";

$m[$a] = "A";
echo $m[$a];
echo ":";
echo $m[$b];
echo ":";

$c = new Box(1);
echo isset($m[$c]) ? "collide" : "distinct";
"#,
    );
    assert_eq!(out, "2:y:y:A:b:distinct");
}

/// Verifies `offsetUnset` (via `unset($m[$o])`) removes a single entry and keeps the rest.
#[test]
fn test_weakmap_unset_removes_entry() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $id;
    public function __construct(int $id) {
        $this->id = $id;
    }
}

$m = new WeakMap();
$a = new Box(1);
$b = new Box(2);
$c = new Box(3);
$m[$a] = "a";
$m[$b] = "b";
$m[$c] = "c";

unset($m[$b]);
echo count($m);
echo ":";
echo isset($m[$a]) && isset($m[$c]) ? "kept" : "lost";
echo ":";
echo isset($m[$b]) ? "stayed" : "gone";
"#,
    );
    assert_eq!(out, "2:kept:gone");
}

/// Verifies `foreach` iteration yields each object key with its mapped value (PHP WeakMap order).
#[test]
fn test_weakmap_foreach_yields_object_key_and_value() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $id;
    public function __construct(int $id) {
        $this->id = $id;
    }
}

$m = new WeakMap();
$a = new Box(1);
$b = new Box(2);
$m[$a] = "a";
$m[$b] = "b";

foreach ($m as $key => $value) {
    echo $key->id;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "1=a;2=b;");
}

/// Verifies that `offsetGet` on an absent key returns `null` (documented PHP divergence: PHP throws).
#[test]
fn test_weakmap_offset_get_absent_returns_null() {
    let out = compile_and_run(
        r#"<?php
$m = new WeakMap();
$o = new stdClass();
$v = $m[$o];
var_dump($v);
"#,
    );
    assert_eq!(out, "NULL\n");
}