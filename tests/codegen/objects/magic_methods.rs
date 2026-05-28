//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object magic methods, including magic tostring supports echo concat and cast, magic tostring missing method is runtime fatal, and magic get handles missing property reads.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

/// Verifies `__toString` result is usable in direct `echo`, string concatenation, and explicit `(string)` cast.
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

/// Verifies that a class without `__toString` causes a runtime fatal error when echoed.
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

/// Verifies `__get` is invoked for undefined property reads, returning the intercepted name.
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

/// Verifies the type checker merges `__get` return types across top-level branches so that
/// concatenating two successive calls with different internal-state return types works correctly.
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

/// Verifies `__set` is invoked for undefined property writes, capturing the name and value.
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

/// Verifies `__get` and `__set` interact correctly: a write via `__set` is readable via `__get`.
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

/// Verifies `__invoke` is called when an object is invoked as a function via a variable holding the object.
#[test]
fn test_magic_invoke_handles_variable_object_call() {
    let out = compile_and_run(
        r#"<?php
class CallableObj {
    public function __invoke($x) { return $x * 2; }
}
$obj = new CallableObj();
echo $obj(21);
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies `__invoke` is called when an object is invoked as a function via a loaded expression (`new`).
#[test]
fn test_magic_invoke_handles_loaded_object_expr_call() {
    let out = compile_and_run(
        r#"<?php
class CallableObj {
    public function __invoke($x) { return $x * 2; }
}
echo (new CallableObj())(21);
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies `__call` is invoked for undefined methods, receiving the method name and argument list.
#[test]
fn test_magic_call_handles_missing_method() {
    let out = compile_and_run(
        r#"<?php
class Proxy {
    public function __call($method, $args) {
        return "called:" . $method . ":" . implode(",", $args);
    }
}
$p = new Proxy();
echo $p->doSomething(1, 2, 3);
"#,
    );
    assert_eq!(out, "called:doSomething:1,2,3");
}

// =============================================================================
// Non-class regression edge cases
// =============================================================================
