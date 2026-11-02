//! Purpose:
//! Regression tests for runtime property names whose static type is a nullable or scalar union.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (via `property_access/mod.rs`).
//!
//! Key details:
//! - php converts a `null`, bool, float, int or string runtime name to a string for
//!   `$object->{$name}`. The checker used to accept only `string`, `int` and `mixed`, which
//!   refused Laravel's `ReadsClassAttributes::getAttributeValue(?string $property)`.
//! - Expected output is real php 8.5 output (deprecations are silenced with `@`).

use super::*;

/// Verifies reads, probes and writes through `?string` and `int|string` property names.
#[test]
fn test_dynamic_property_name_accepts_nullable_and_union_types() {
    let out = compile_and_run(
        r#"<?php
class T {
    public $name = "n";
    public int $count = 3;
    public function get(?string $property, $default = null) {
        if (isset($this->{$property}) && $this->{$property} !== "zz") {
            return $this->{$property};
        }
        return $this->{$property} ?? $default;
    }
    public function put(?string $property, $v) { @$this->{$property} = $v; }
    public function pi(int|string $p) { return $this->{$p} ?? "none"; }
}
$t = new T;
echo $t->get("name"), "|", $t->get("count"), "|", var_export($t->get("missing", "dflt"), true), "|", var_export($t->get(null, "nulldef"), true), "\n";
$t->put("extra", 5);
$t->put(null, 6);
echo $t->get("extra"), "|", $t->get(null), "\n";
echo $t->pi("name"), $t->pi("nope"), "\n";
"#,
    );
    assert_eq!(out, "n|3|'dflt'|'nulldef'\n5|6\nnnone\n");
}

/// Verifies a trait method walking an unreachable empty-array element still accepts the name.
///
/// Laravel's `HasUniqueIds::setUniqueIds()` does `foreach ($this->uniqueIds() as $column)
/// { $this->{$column} ... }`, and the base `uniqueIds()` returns `[]`, whose element type the
/// checker could not name as a string.
#[test]
fn test_dynamic_property_name_over_empty_array_element_in_trait() {
    let out = compile_and_run(
        r#"<?php
trait HasU {
    public function uniqueIds() { return []; }
    public function setIds() {
        foreach ($this->uniqueIds() as $column) {
            if (empty($this->{$column})) {
                $this->{$column} = 'x';
            }
        }
    }
}
class M { use HasU; public $id = null; }
class N { use HasU; public $uuid = null; public function uniqueIds() { return ['uuid']; } }
$m = new M; $m->setIds(); var_dump($m->id);
$n = new N; $n->setIds(); var_dump($n->uuid);
"#,
    );
    assert_eq!(out, "NULL\nstring(1) \"x\"\n");
}
