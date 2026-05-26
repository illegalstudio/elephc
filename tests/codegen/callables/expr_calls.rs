//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of callables expr calls, including expr call returns string, expr call returns float, and expr call returns integer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Verifies that expr call returns string.
#[test]
fn test_expr_call_returns_string() {
    // Verifies that a callable variable returning a string compiles and runs correctly.
    // Fixture: anonymous function taking $name, returning `"Hello " . $name`.
    let out = compile_and_run(
        r#"<?php
$greet = function($name) { return "Hello " . $name; };
echo $greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

/// Verifies that expr call returns float.
#[test]
fn test_expr_call_returns_float() {
    // Verifies that a callable variable returning a float compiles and runs correctly.
    // Fixture: anonymous function returning `$x * 3.14`, called with `2.0`.
    let out = compile_and_run(
        r#"<?php
$calc = function($x) { return $x * 3.14; };
echo $calc(2.0);
"#,
    );
    assert_eq!(out, "6.28");
}

/// Verifies that expr call returns integer.
#[test]
fn test_expr_call_returns_int() {
    // Verifies that a callable variable returning an integer compiles and runs correctly.
    // Fixture: anonymous function returning `$x * 2`, called with `21`.
    let out = compile_and_run(
        r#"<?php
$double = function($x) { return $x * 2; };
echo $double(21);
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that expr call string in concat.
#[test]
fn test_expr_call_string_in_concat() {
    // Verifies that a callable variable returning a string can be used in concatenation.
    // Fixture: lambda returning `"<b>" . $s . "</b>"`, result prepended with `"Result: "`.
    let out = compile_and_run(
        r#"<?php
$tag = function($s) { return "<b>" . $s . "</b>"; };
echo "Result: " . $tag("hello");
"#,
    );
    assert_eq!(out, "Result: <b>hello</b>");
}

/// Verifies that closure call returns string.
#[test]
fn test_closure_call_returns_string() {
    // Verifies that a closure stored in a variable can be called and returns a string.
    // Fixture: `$fn = function() { return "test"; };` called as `$fn()`.
    let out = compile_and_run(
        r#"<?php
$fn = function() { return "test"; };
$result = $fn();
echo $result;
"#,
    );
    assert_eq!(out, "test");
}

/// Verifies that closure via array element local preserves signature.
#[test]
fn test_closure_via_array_element_local_preserves_signature() {
    // Verifies that a closure fetched from an array element and stored in a local
    // preserves its call signature and returns the expected value.
    // Fixture: closure stored in `$arr[0]`, fetched into `$f`, called with `"2"`.
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

/// Verifies that callable by ref parameter dereferences descriptor before call.
#[test]
fn test_callable_by_ref_parameter_dereferences_descriptor_before_call() {
    let out = compile_and_run(
        r#"<?php
function run(callable &$cb) {
    echo $cb(4);
}
$cb = function($n) { return $n + 3; };
run($cb);
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies that closure via function parameter preserves signature.
#[test]
fn test_closure_via_function_parameter_preserves_signature() {
    // Verifies that a closure passed as a function argument can be called inside
    // that function and returns the correct value.
    // Fixture: closure captured by `$ok`, passed to `call_it()`, invoked with `"4"`.
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

/// Verifies that fcc variable function target direct call.
#[test]
fn test_fcc_variable_function_target_direct_call() {
    // Verifies that a user-defined function used as FCC target calls correctly via direct call syntax.
    // Fixture: `function triple(int $n): int { return $n * 3; }` called as `$cb(14)` where `$cb = triple(...)`.
    let out = compile_and_run(
        r#"<?php
function triple(int $n): int { return $n * 3; }
$cb = triple(...);
echo $cb(14);
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that fcc variable builtin function target direct call.
#[test]
fn test_fcc_variable_builtin_function_target_direct_call() {
    // Verifies that a builtin function used as FCC target calls correctly via direct call syntax.
    // Fixture: `$cb = strtoupper(...)`, invoked with `"hello"`.
    let out = compile_and_run(
        r#"<?php
$cb = strtoupper(...);
echo $cb("hello");
"#,
    );
    assert_eq!(out, "HELLO");
}

/// Verifies that fcc variable reassignment clears target.
#[test]
fn test_fcc_variable_reassignment_clears_target() {
    // Verifies that reassigning an FCC variable to a regular closure causes subsequent
    // calls to use the closure wrapper path, not the stale Function short-circuit.
    // Fixture: `$cb` first bound to `double(...)`, then rebound to an anonymous function;
    // final call `$cb(5)` returns `5 + 100 = 105`.
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

/// Verifies that fcc variable via pipe short circuits.
#[test]
fn test_fcc_variable_via_pipe_short_circuits() {
    // Verifies that an FCC variable can be called via the pipe operator with correct short-circuit behavior.
    // Fixture: `$cb = quad(...)` (function returning `$n * 4`), called as `6 |> $cb`.
    let out = compile_and_run(
        r#"<?php
function quad(int $n): int { return $n * 4; }
$cb = quad(...);
echo 6 |> $cb;
"#,
    );
    assert_eq!(out, "24");
}

/// Verifies that fcc variable instance method target direct call.
#[test]
fn test_fcc_variable_instance_method_target_direct_call() {
    // Verifies that an instance method FCC target calls correctly via direct call syntax.
    // Fixture: `$b = new Bumper(10)`, `$cb = $b->apply(...)`, `$cb(5)` returns `5 + 10`.
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

/// Verifies that fcc variable instance method via pipe short circuits.
#[test]
fn test_fcc_variable_instance_method_via_pipe_short_circuits() {
    // Verifies that an instance method FCC variable can be called via the pipe operator with correct short-circuit.
    // Fixture: `$m = new Multiplier(7)`, `$cb = $m->times(...)`, `6 |> $cb` returns `42`.
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

/// Verifies that fcc variable static method named target direct call.
#[test]
fn test_fcc_variable_static_method_named_target_direct_call() {
    // Verifies that a statically-namespaced method FCC target (Class::method(...)) calls correctly.
    // Fixture: `$cb = Calc::quad(...)`, `$cb(5)` returns `20`.
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

/// Verifies that fcc variable static method via pipe short circuits.
#[test]
fn test_fcc_variable_static_method_via_pipe_short_circuits() {
    // Verifies that a statically-namespaced method FCC target can be called via the pipe operator.
    // Fixture: `$cb = Calc::add10(...)`, `7 |> $cb` returns `17`.
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

/// Verifies that fcc variable self static method target short circuits.
#[test]
fn test_fcc_variable_self_static_method_target_short_circuits() {
    // Verifies that `self::method(...)` FCC short-circuits to the lexically enclosing class method.
    // Fixture: `Marker::wrap()` creates `$cb = self::tag(...)`, calls `$cb("ok")` → `"[ok]"`.
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

/// Verifies that fcc variable parent static method target short circuits.
#[test]
fn test_fcc_variable_parent_static_method_target_short_circuits() {
    // Verifies that `parent::method(...)` FCC short-circuits to the parent class method.
    // Fixture: `Child::viaParent()` creates `$cb = parent::name(...)`, calls `$cb()` → `"Base"`.
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

/// Verifies that fcc variable static receiver short circuits with late static binding.
#[test]
fn test_fcc_variable_static_receiver_short_circuits_with_late_static_binding() {
    // Verifies that `static::method(...)` FCC preserves late static binding via the called-class chain
    // (reuses `__elephc_called_class_id` without a closure trampoline). Fixture: `B::describe()` calls
    // `static::name()` through an FCC; B's override is selected, returning `"B"`.
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

/// Verifies that fcc variable static receiver in instance method resolves via this.
#[test]
fn test_fcc_variable_static_receiver_in_instance_method_resolves_via_this() {
    // Verifies that `static::method(...)` FCC created inside an instance method resolves via `$this`
    // at runtime (falls through `__elephc_fcc_called_class_id` → `__elephc_called_class_id` →
    // `__elephc_fcc_this` → `this`). Fixture: `B::describe()` returns `"B"` via late static binding.
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

/// Verifies that fcc variable static receiver chained pipe.
#[test]
fn test_fcc_variable_static_receiver_chained_pipe() {
    // Verifies that `static::method(...)` FCC combined with the pipe operator works correctly.
    // Fixture: `Caps::go("ok")` creates `$cb = static::shout(...)`, pipes `$s |> $cb` → `"OK"`.
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

/// Verifies that fcc indirect via array element through local runs.
#[test]
fn test_fcc_indirect_via_array_element_through_local_runs() {
    // Verifies that an FCC stored in an array element and fetched back into a local
    // still calls correctly (goes through the closure wrapper fallback path).
    // Fixture: `tripler(...)` stored in `$arr[0]`, fetched into `$cb`, called with `7`.
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

/// Verifies that fcc method complex receiver via local workaround runs.
#[test]
fn test_fcc_method_complex_receiver_via_local_workaround_runs() {
    // Verifies the documented workaround for complex receiver FCC: copy the chained receiver to a
    // local first, then create the FCC against that local. Fixture: `$inner = $o->inner`,
    // `$cb = $inner->apply(...)`, piped call `7 |> $cb` returns `17`.
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

/// Verifies that fcc indirect via assoc array value through local runs.
#[test]
fn test_fcc_indirect_via_assoc_array_value_through_local_runs() {
    // Verifies that an FCC stored in an associative-array value and fetched back into a local
    // calls correctly (closure wrapper fallback path). Fixture: `doubler(...)` stored as
    // `["double" => doubler(...)]`, fetched via `$reg["double"]`, called with `9`.
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

/// Verifies that closure fetched from object property through method runs.
#[test]
fn test_closure_fetched_from_object_property_through_method_runs() {
    // Verifies that a closure stored in an object property and fetched via a method call
    // can still be invoked correctly. Fixture: `Box` holds `$cb`; `fetch()` returns it;
    // `invoke($b->fetch(), 5)` returns `5 + 7 = 12`.
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

/// Verifies that fcc variable static method named target preserves late static binding.
#[test]
fn test_fcc_variable_static_method_named_target_preserves_late_static_binding() {
    // Verifies that a Named static-method FCC target (B::describe(...)) preserves late static
    // binding when the FCC is called. Fixture: `B::describe()` calls `static::name()` returning `"B"`.
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
