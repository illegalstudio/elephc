//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object magic methods, including magic tostring supports echo concat and cast, magic tostring missing method is runtime fatal, and magic get handles missing property reads.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

#[test]
fn test_magic_tostring_supports_echo_concat_and_cast() {
    let out = compile_and_run(
        r#"<?php
class User {
    public $name;
    public function __construct($name) { $this->name = $name; }
    public function __toString() { return "@" . $this->name; }
}
$u = new User("nahime");
echo $u;
echo "|" . $u;
echo "|" . (string)$u;
"#,
    );
    assert_eq!(out, "@nahime|@nahime|@nahime");
}

#[test]
fn test_magic_tostring_missing_method_is_runtime_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Plain {}
$p = new Plain();
echo $p;
"#,
    );
    assert!(err.contains("could not be converted to string"), "{err}");
}

#[test]
fn test_magic_get_handles_missing_property_reads() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public function __get($name) {
        return "[" . $name . "]";
    }
}
$b = new Bag();
echo $b->title . "|" . $b->slug;
"#,
    );
    assert_eq!(out, "[title]|[slug]");
}

#[test]
fn test_magic_get_merges_return_types_across_top_level_branches() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public $flip = false;
    public function __get($name) {
        if ($this->flip) {
            return "[" . $name . "]";
        }
        $this->flip = true;
        return 123;
    }
}
$b = new Bag();
echo $b->id . "|" . $b->slug;
"#,
    );
    assert_eq!(out, "123|[slug]");
}

#[test]
fn test_magic_set_handles_missing_property_writes() {
    let out = compile_and_run(
        r#"<?php
class Recorder {
    public $log = "";
    public function __set($name, $value) {
        $this->log = $this->log . $name . "=" . $value . ";";
    }
}
$r = new Recorder();
$r->count = 42;
$r->label = "ok";
echo $r->log;
"#,
    );
    assert_eq!(out, "count=42;label=ok;");
}

#[test]
fn test_magic_get_and_set_can_work_together() {
    let out = compile_and_run(
        r#"<?php
class Meta {
    public $last = "";
    public function __set($name, $value) { $this->last = $name . ":" . $value; }
    public function __get($name) { return $this->last . "|" . $name; }
}
$m = new Meta();
$m->answer = 99;
echo $m->answer;
"#,
    );
    assert_eq!(out, "answer:99|answer");
}

// =============================================================================
// Non-class regression edge cases
// =============================================================================
