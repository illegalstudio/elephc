//! Purpose:
//! Checks receiver cleanup when retiring a property source invokes a throwing destructor.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - The new field must be published before the source destructor runs.
//! - The receiver stays protected after the source cleanup pin is detached.

use crate::support::compile_and_run_with_heap_debug;

/// Checks publication order, caught exceptions, and every owner after repeated throws.
fn assert_clean(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Keeps the nested nullable source destructor receiver owned during source cleanup.
#[test]
fn test_property_nested_nullable_source_destructor() {
    assert_clean(
        r#"<?php
class Label {
    public function __construct(private string $value) {}
    public function __toString(): string { return $this->value; }
    public function __destruct() { throw new Exception("source gone"); }
}
class Box { public string $s = "seed"; }
class Holder { public ?Box $next = null; }
function make_label(int $i): mixed { return new Label("label" . $i); }
function run_case(): void {
    $h = new Holder();
    $h->next = new Box();
    $caught = 0;
    for ($i = 0; $i < 40; $i++) {
        try { $h->next->s = make_label($i); }
        catch (Exception $error) { $caught++; unset($error); }
    }
    echo $caught, ":", $h->next->s, "\n";
}
run_case();
"#,
        "40:label39\n",
    );
}

/// Keeps the nested mixed source destructor receiver owned during source cleanup.
#[test]
fn test_property_nested_mixed_source_destructor() {
    assert_clean(
        r#"<?php
class Label {
    public function __construct(private string $value) {}
    public function __toString(): string { return $this->value; }
    public function __destruct() { throw new Exception("source gone"); }
}
class Box { public string $s = "seed"; }
class Holder { public mixed $next = null; }
function make_label(int $i): mixed { return new Label("label" . $i); }
function run_case(): void {
    $h = new Holder();
    $h->next = new Box();
    $caught = 0;
    for ($i = 0; $i < 40; $i++) {
        try { $h->next->s = make_label($i); }
        catch (Exception $error) { $caught++; unset($error); }
    }
    echo $caught, ":", $h->next->s, "\n";
}
run_case();
"#,
        "40:label39\n",
    );
}
