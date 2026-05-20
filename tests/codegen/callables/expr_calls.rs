//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of callables expr calls, including expr call returns string, expr call returns float, and expr call returns integer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

#[test]
fn test_expr_call_returns_string() {
    let out = compile_and_run(
        r#"<?php
$greet = function($name) { return "Hello " . $name; };
echo $greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_expr_call_returns_float() {
    let out = compile_and_run(
        r#"<?php
$calc = function($x) { return $x * 3.14; };
echo $calc(2.0);
"#,
    );
    assert_eq!(out, "6.28");
}

#[test]
fn test_expr_call_returns_int() {
    let out = compile_and_run(
        r#"<?php
$double = function($x) { return $x * 2; };
echo $double(21);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_expr_call_string_in_concat() {
    let out = compile_and_run(
        r#"<?php
$tag = function($s) { return "<b>" . $s . "</b>"; };
echo "Result: " . $tag("hello");
"#,
    );
    assert_eq!(out, "Result: <b>hello</b>");
}

#[test]
fn test_closure_call_returns_string() {
    let out = compile_and_run(
        r#"<?php
$fn = function() { return "test"; };
$result = $fn();
echo $result;
"#,
    );
    assert_eq!(out, "test");
}

#[test]
fn test_closure_via_array_element_local_preserves_signature() {
    let out = compile_and_run(
        r#"<?php
$arr = [];
$arr[] = function($n) { return "v" . $n; };
$f = $arr[0];
echo $f("2");
"#,
    );
    assert_eq!(out, "v2");
}

#[test]
fn test_closure_via_function_parameter_preserves_signature() {
    let out = compile_and_run(
        r#"<?php
$ok = function($n) { return "v" . $n; };
function call_it($fn) { return $fn("4"); }
echo call_it($ok);
"#,
    );
    assert_eq!(out, "v4");
}

// --- First-class callable variable short-circuit (PHP 8.5 pipe opt) ---

#[test]
fn test_fcc_variable_function_target_direct_call() {
    let out = compile_and_run(
        r#"<?php
function triple(int $n): int { return $n * 3; }
$cb = triple(...);
echo $cb(14);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_fcc_variable_builtin_function_target_direct_call() {
    let out = compile_and_run(
        r#"<?php
$cb = strtoupper(...);
echo $cb("hello");
"#,
    );
    assert_eq!(out, "HELLO");
}

#[test]
fn test_fcc_variable_reassignment_clears_target() {
    // $cb is rebound to a generic closure, so subsequent calls must go through
    // the regular closure path (not the stale Function short-circuit).
    let out = compile_and_run(
        r#"<?php
function double(int $n): int { return $n * 2; }
$cb = double(...);
$cb = function (int $n): int { return $n + 100; };
echo $cb(5);
"#,
    );
    assert_eq!(out, "105");
}

#[test]
fn test_fcc_variable_via_pipe_short_circuits() {
    let out = compile_and_run(
        r#"<?php
function quad(int $n): int { return $n * 4; }
$cb = quad(...);
echo 6 |> $cb;
"#,
    );
    assert_eq!(out, "24");
}

#[test]
fn test_fcc_variable_instance_method_target_direct_call() {
    let out = compile_and_run(
        r#"<?php
class Bumper {
    private int $bump;
    public function __construct(int $bump) { $this->bump = $bump; }
    public function apply(int $n): int { return $n + $this->bump; }
}
$b = new Bumper(10);
$cb = $b->apply(...);
echo $cb(5);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_fcc_variable_instance_method_via_pipe_short_circuits() {
    let out = compile_and_run(
        r#"<?php
class Multiplier {
    public function __construct(private int $factor) {}
    public function times(int $n): int { return $n * $this->factor; }
}
$m = new Multiplier(7);
$cb = $m->times(...);
echo 6 |> $cb;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_fcc_variable_static_method_named_target_direct_call() {
    let out = compile_and_run(
        r#"<?php
class Calc {
    public static function quad(int $n): int { return $n * 4; }
}
$cb = Calc::quad(...);
echo $cb(5);
"#,
    );
    assert_eq!(out, "20");
}

#[test]
fn test_fcc_variable_static_method_via_pipe_short_circuits() {
    let out = compile_and_run(
        r#"<?php
class Calc {
    public static function add10(int $n): int { return $n + 10; }
}
$cb = Calc::add10(...);
echo 7 |> $cb;
"#,
    );
    assert_eq!(out, "17");
}

#[test]
fn test_fcc_variable_self_static_method_target_short_circuits() {
    // `self::method(...)` is resolved to the lexically-current class at FCC
    // storage time, so the short-circuit fires and calls the right method.
    let out = compile_and_run(
        r#"<?php
class Marker {
    public static function tag(string $s): string { return "[" . $s . "]"; }
    public static function wrap(string $s): string {
        $cb = self::tag(...);
        return $cb($s);
    }
}
echo Marker::wrap("ok");
"#,
    );
    assert_eq!(out, "[ok]");
}

#[test]
fn test_fcc_variable_parent_static_method_target_short_circuits() {
    // `parent::method(...)` resolves to the parent class at FCC storage time.
    let out = compile_and_run(
        r#"<?php
class Base {
    public static function name(): string { return "Base"; }
}
class Child extends Base {
    public static function name(): string { return "Child"; }
    public static function viaParent(): string {
        $cb = parent::name(...);
        return $cb();
    }
}
echo Child::viaParent();
"#,
    );
    assert_eq!(out, "Base");
}

#[test]
fn test_fcc_variable_static_receiver_short_circuits_with_late_static_binding() {
    // `static::method(...)` short-circuits to a direct static-method call that
    // re-uses the caller scope's hidden `__elephc_called_class_id` (the same
    // chain `emit_forwarded_called_class_id` would consult inside the wrapper),
    // so late-static binding is preserved without the closure trampoline.
    let out = compile_and_run(
        r#"<?php
class A {
    public static function name(): string { return "A"; }
    public static function describe(): string {
        $cb = static::name(...);
        return $cb();
    }
}
class B extends A {
    public static function name(): string { return "B"; }
}
echo B::describe();
"#,
    );
    assert_eq!(out, "B");
}

#[test]
fn test_fcc_variable_static_receiver_in_instance_method_resolves_via_this() {
    // When `$cb = static::name(...)` is created inside an instance method, the
    // caller scope's `$this` provides the runtime called class. The short-circuit
    // falls through the same `__elephc_fcc_called_class_id` → `__elephc_called_class_id`
    // → `__elephc_fcc_this` → `this` chain as the closure wrapper would.
    let out = compile_and_run(
        r#"<?php
class A {
    public static function name(): string { return "A"; }
    public function describe(): string {
        $cb = static::name(...);
        return $cb();
    }
}
class B extends A {
    public static function name(): string { return "B"; }
}
$obj = new B();
echo $obj->describe();
"#,
    );
    assert_eq!(out, "B");
}

#[test]
fn test_fcc_variable_static_receiver_chained_pipe() {
    // Combine commit 1 (pipe operator) with the static:: short-circuit.
    let out = compile_and_run(
        r#"<?php
class Caps {
    public static function shout(string $s): string { return strtoupper($s); }
    public static function go(string $s): string {
        $cb = static::shout(...);
        return $s |> $cb;
    }
}
echo Caps::go("ok");
"#,
    );
    assert_eq!(out, "OK");
}

// --- Defensive: FCC stored outside a local (array/property) ---
// The optimisation only tracks FCC targets for direct local assignments. When the
// callable is materialised in an array element or an object property, retrieving
// it back into a local goes through the closure wrapper path (the assignment
// `$cb = $arr[0]` resets `first_class_callable_targets[$cb]` because the RHS is
// `ExprKind::ArrayAccess`, not a `FirstClassCallable` or callable `Variable`).
// These tests pin the runtime behaviour so a future refinement that extends the
// tracking to those storages cannot accidentally regress the fallback semantics.

#[test]
fn test_fcc_indirect_via_array_element_through_local_runs() {
    let out = compile_and_run(
        r#"<?php
function tripler(int $n): int { return $n * 3; }
$arr = [tripler(...)];
$cb = $arr[0];
echo $cb(7);
"#,
    );
    assert_eq!(out, "21");
}

#[test]
fn test_fcc_method_complex_receiver_via_local_workaround_runs() {
    // FCC creation rejects complex receiver expressions for instance methods
    // (`$obj->inner->method(...)`). The documented workaround is to copy the
    // chained receiver to a local first, then take the FCC against that local.
    let out = compile_and_run(
        r#"<?php
class Inner {
    public function __construct(private int $bump) {}
    public function apply(int $n): int { return $n + $this->bump; }
}
class Outer {
    public Inner $inner;
    public function __construct(int $bump) { $this->inner = new Inner($bump); }
}
$o = new Outer(10);
$inner = $o->inner;
$cb = $inner->apply(...);
echo 7 |> $cb;
"#,
    );
    assert_eq!(out, "17");
}

#[test]
fn test_fcc_indirect_via_assoc_array_value_through_local_runs() {
    // Storing the FCC in an associative-array value, fetching it back into a
    // local, and invoking — exercises the same "FCC outside a local slot →
    // closure wrapper fallback" path as direct array indexing.
    let out = compile_and_run(
        r#"<?php
function doubler(int $n): int { return $n * 2; }
$reg = ["double" => doubler(...)];
$cb = $reg["double"];
echo $cb(9);
"#,
    );
    assert_eq!(out, "18");
}

#[test]
fn test_closure_fetched_from_object_property_through_method_runs() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public $cb;

    public function __construct($cb) {
        $this->cb = $cb;
    }

    public function fetch() {
        return $this->cb;
    }
}

function invoke($cb, $x) {
    return $cb($x);
}

$b = new Box(function($x) { return $x + 7; });
echo invoke($b->fetch(), 5);
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_fcc_variable_static_method_named_target_preserves_late_static_binding() {
    // `B::m(...)` produces a Named target. The short-circuit calls B::m
    // directly, and `static::name()` inside m is resolved at call time so it
    // still picks B's override.
    let out = compile_and_run(
        r#"<?php
class A {
    public static function name(): string { return "A"; }
    public static function describe(): string { return static::name(); }
}
class B extends A {
    public static function name(): string { return "B"; }
}
$cb = B::describe(...);
echo $cb();
"#,
    );
    assert_eq!(out, "B");
}
