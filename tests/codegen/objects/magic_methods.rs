//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object magic methods, including `__toString`, `__get`/`__set`, `__call`/`__callStatic`, `__isset`/`__unset`, and `__invoke`.
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

/// Verifies `__callStatic` is invoked for undefined static methods, receiving the
/// method name and argument list.
#[test]
fn test_magic_callstatic_handles_missing_static_method() {
    let out = compile_and_run(
        r#"<?php
class Router {
    public static function __callStatic($name, $args) {
        return "static:" . $name . "(" . implode(",", $args) . ")";
    }
}
echo Router::get("/home", "x");
"#,
    );
    assert_eq!(out, "static:get(/home,x)");
}

/// Verifies a parent's `__callStatic` handles undefined static calls made through
/// a subclass, including `count($args)` over the forwarded argument list.
#[test]
fn test_magic_callstatic_inherited_by_subclass() {
    let out = compile_and_run(
        r#"<?php
abstract class Model {
    public static function __callStatic($method, $args) {
        return $method . "(" . count($args) . ")";
    }
}
class User extends Model {}
echo User::where("active", 1), "|", User::first();
"#,
    );
    assert_eq!(out, "where(2)|first(0)");
}

/// Verifies `isset($obj->prop)` on an undeclared property dispatches to `__isset`
/// and uses its boolean result for both present and absent names.
#[test]
fn test_magic_isset_handles_undeclared_property() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public function __isset($name) {
        return $name === "present";
    }
}
$b = new Bag();
echo isset($b->present) ? "yes" : "no";
echo "|";
echo isset($b->absent) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes|no");
}

/// Verifies `unset($obj->prop)` on an undeclared property dispatches to `__unset`.
#[test]
fn test_magic_unset_handles_undeclared_property() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public function __unset($name) {
        echo "unset:" . $name;
    }
}
$b = new Bag();
unset($b->token);
"#,
    );
    assert_eq!(out, "unset:token");
}

/// Verifies `__get`/`__isset`/`__unset` cooperate over a scalar-backed virtual
/// property, mirroring PHP's interception of reads, existence checks, and removal.
#[test]
fn test_magic_isset_unset_get_virtual_property() {
    let out = compile_and_run(
        r#"<?php
class Session {
    private bool $tokenPresent = true;
    private string $tokenValue = "abc123";
    public function __get($k) { return $k === "token" ? $this->tokenValue : ""; }
    public function __isset($k) { return $k === "token" && $this->tokenPresent; }
    public function __unset($k) { if ($k === "token") { $this->tokenPresent = false; } }
}
$s = new Session();
echo isset($s->token) ? "has:" . $s->token : "none";
echo "|";
unset($s->token);
echo isset($s->token) ? "has:" . $s->token : "none";
"#,
    );
    assert_eq!(out, "has:abc123|none");
}

/// Verifies `empty($obj->prop)` on an overloaded (magic) property consults
/// `__isset` before `__get`, matching PHP. An unset virtual property is empty
/// without `__get` ever running; a set property reflects its value's emptiness
/// (including the `0` falsy case). Regression: `empty` previously evaluated
/// `__get` and checked only its truthiness, ignoring `__isset` entirely.
#[test]
fn test_empty_on_virtual_property_consults_isset() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public function __get($k) { return $k === "zero" ? 0 : 7; }
    public function __isset($k) { return $k !== "gone"; }
}
$b = new Bag();
var_dump(empty($b->seven));  // __isset true, __get 7 -> not empty
var_dump(empty($b->zero));   // __isset true, __get 0 -> empty
var_dump(empty($b->gone));   // __isset false -> empty, __get not called
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\nbool(true)\n");
}

/// Verifies `isset()` and `empty()` evaluate to `bool` (not `int`), both on a
/// magic property and a plain variable, matching PHP's `var_dump` rendering.
#[test]
fn test_isset_empty_return_bool() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public function __get($k) { return 1; }
    public function __isset($k) { return $k === "set"; }
}
$b = new Bag();
$x = 5;
var_dump(isset($b->set));
var_dump(isset($b->nope));
var_dump(isset($x));
var_dump(empty($x));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(true)\nbool(false)\n");
}

/// Compiles and runs the checked-in `examples/magic-methods/main.php` fixture,
/// exercising every supported magic method end to end.
#[test]
fn test_example_magic_methods_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/magic-methods/main.php"));
    assert_eq!(
        out,
        "@nahime\n[missing]\nrole=admin;visits=3;\nnahime:active\nmissing displayName(short)\nactive\ninactive\nstatic create(nahime)\n"
    );
}

// =============================================================================
// Non-class regression edge cases
// =============================================================================
