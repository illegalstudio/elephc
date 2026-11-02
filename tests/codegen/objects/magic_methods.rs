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

/// Verifies a value of the DECLARED type `object` reaches `__toString` in every string context,
/// and that a subclass's `__toString` binds through a parent-typed value.
///
/// `object` is spelled `PhpType::Object("")` in codegen, so the class-name lookup that binds
/// `__toString` found nothing and every one of these contexts wrote
/// `Object of class  could not be converted to string` — with an empty class name — and exited.
/// The parent-typed case failed the same lookup for the opposite reason: `Base` really has no
/// `__toString`, but the value is a `Child` that does, and PHP calls it.
///
/// Values are `php -n` 8.5.6 verbatim: `xL!|yL!|L!|L!|L!|L!|Z|pL!|xC!|C!|C!`.
#[test]
fn test_declared_object_type_and_subclass_reach_tostring_in_string_contexts() {
    let out = compile_and_run(
        r#"<?php
class Label { public function __toString(): string { return "L!"; } }
class Base { public $v = 1; }
class Child extends Base { public function __toString(): string { return "C!"; } }

function asObject(): object { return new Label(); }
function takeObject(object $o): string { return "p" . $o; }
function asBase(): Base { return new Child(); }

$o = asObject();
echo "x" . $o, "|", "y$o", "|", (string) $o, "|", strval($o), "|", sprintf('%s', $o), "|";
echo $o;
echo "|", str_replace("L!", "Z", $o), "|", takeObject(new Label()), "|";
$b = asBase();
echo "x" . $b, "|", (string) $b, "|";
echo $b;
"#,
    );
    assert_eq!(out, "xL!|yL!|L!|L!|L!|L!|Z|pL!|xC!|C!|C!");
}

/// Verifies the conversion failure NAMES the runtime class even when the static type is `object`,
/// and that output written before it survives.
///
/// The `object`-typed arm used to print an EMPTY class name, and the boxed-Mixed arm printed no
/// class at all (`Fatal error: Object could not be converted to string`), which is what a Symfony
/// `--web` request died with. Both now resolve the name from the object itself.
#[test]
fn test_declared_object_conversion_error_names_the_runtime_class() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Plain { public $v = 1; }
function asObject(): object { return new Plain(); }
echo "before\n";
echo "x" . asObject();
"#,
    );
    assert!(
        err.contains("Object of class Plain could not be converted to string"),
        "{err}"
    );
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

/// Verifies `__isset` is invoked for undefined property probes and its result is coerced to bool.
#[test]
fn test_magic_isset_handles_missing_property_probes() {
    let out = compile_and_run(
        r#"<?php
class Probe {
    public function __isset($name) {
        echo "check:" . $name . ";";
        if ($name === "enabled") {
            return "yes";
        }
        return "";
    }
}
$p = new Probe();
echo isset($p->enabled) ? "Y" : "N";
echo ":";
echo isset($p->disabled) ? "Y" : "N";
"#,
    );
    assert_eq!(out, "check:enabled;Y:check:disabled;N");
}

/// Verifies `isset()` does not read an undefined property through `__get`.
#[test]
fn test_magic_isset_missing_property_does_not_call_magic_get() {
    let out = compile_and_run(
        r#"<?php
class Probe {
    public function __get($name) {
        echo "get:" . $name . ";";
        return 1;
    }
}
$p = new Probe();
echo isset($p->missing) ? "Y" : "N";
"#,
    );
    assert_eq!(out, "N");
}

/// Verifies `__isset` is invoked for inaccessible property probes from outside the class.
#[test]
fn test_magic_isset_handles_inaccessible_property_probes() {
    let out = compile_and_run(
        r#"<?php
class Secret {
    private $token = 1;
    public function __isset($name) {
        echo "magic:" . $name . ";";
        return true;
    }
}
$s = new Secret();
echo isset($s->token) ? "Y" : "N";
"#,
    );
    assert_eq!(out, "magic:token;Y");
}

/// Verifies `__unset` is invoked for undefined property unsets.
#[test]
fn test_magic_unset_handles_missing_property_targets() {
    let out = compile_and_run(
        r#"<?php
class Probe {
    public function __unset($name) {
        echo "unset:" . $name . ";";
    }
}
$p = new Probe();
unset($p->missing);
echo "done";
"#,
    );
    assert_eq!(out, "unset:missing;done");
}

/// Verifies `unset()` does not read an undefined property through `__get`.
#[test]
fn test_magic_unset_missing_property_does_not_call_magic_get() {
    let out = compile_and_run(
        r#"<?php
class Probe {
    public function __get($name) {
        echo "get:" . $name . ";";
        return 1;
    }
}
$p = new Probe();
unset($p->missing);
echo "done";
"#,
    );
    assert_eq!(out, "done");
}

/// Verifies `__unset` is invoked for inaccessible property unsets from outside the class.
#[test]
fn test_magic_unset_handles_inaccessible_property_targets() {
    let out = compile_and_run(
        r#"<?php
class Secret {
    private $token = 1;
    public function __unset($name) {
        echo "magic:" . $name . ";";
    }
}
$s = new Secret();
unset($s->token);
echo "done";
"#,
    );
    assert_eq!(out, "magic:token;done");
}

/// Verifies unsetting an undefined property without `__unset` is a no-op.
#[test]
fn test_magic_unset_missing_property_without_magic_is_noop() {
    let out = compile_and_run(
        r#"<?php
class Plain {}
$p = new Plain();
unset($p->missing);
echo "done";
"#,
    );
    assert_eq!(out, "done");
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

/// Verifies `__callStatic` does not intercept an existing static method.
#[test]
fn test_magic_callstatic_leaves_existing_static_method_direct() {
    let out = compile_and_run(
        r#"<?php
class StaticProxy {
    public static function present() {
        return "present";
    }

    public static function __callStatic($method, $args) {
        return "static:" . $method . ":" . implode(",", $args);
    }
}
echo StaticProxy::present();
echo ":";
echo StaticProxy::doSomething(1, 2, 3);
"#,
    );
    assert_eq!(out, "present:static:doSomething:1,2,3");
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
///
/// The expectation is the example's whole stdout, so editing the example without
/// editing this string fails here. The last four lines are the `__toString()`
/// contexts: `implode()` renders the object element through the same conversion
/// the interpolation, concatenation and `(string)` cast beside it use, which is
/// the shape the boxed-object cast fix restored. Verified byte-identical to host
/// PHP 8.5.10.
#[test]
fn test_example_magic_methods_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/magic-methods/main.php"));
    assert_eq!(
        out,
        concat!(
            "@nahime\n",
            "[missing]\n",
            "role=admin;visits=3;\n",
            "nahime:active\n",
            "nahime:self\n",
            "missing displayName(short)\n",
            "active\n",
            "inactive\n",
            "static create(nahime)\n",
            "implode: plain, 42, @ada, 2.5, 1\n",
            "interp:  @ada\n",
            "concat:  @ada\n",
            "cast:    @ada\n",
        )
    );
}

// =============================================================================
// Non-class regression edge cases
// =============================================================================

/// Verifies `static::method()` resolves against the late-static class when only DESCENDANTS of the
/// calling class declare the method: a protected child-only static method is reachable from the
/// parent's scope, the lexical class that lacks it keeps `__callStatic`, `method_exists()` stays
/// false for it, and an instance without any fallback raises PHP's own undefined-method `Error`.
#[test]
fn test_static_call_reaches_protected_method_only_a_child_declares() {
    let out = compile_and_run(
        r#"<?php
abstract class Base {
    public static function run() { return static::hook() . "|" . static::name(); }
    public static function check() { return method_exists(static::class, 'hook') ? 'yes' : 'no'; }
    public static function withArgs($a, $b) { return static::combine($a, $b); }
    public static function __callStatic($m, $a) { return "callStatic:" . $m . count($a); }
    public static function name() { return "base"; }
}
class Child extends Base {
    protected static function hook() { return "child-hook"; }
    protected static function combine($a, $b) { return $a . "+" . $b . "@" . static::name(); }
    public static function name() { return "child"; }
}
class GrandChild extends Child {
    public static function name() { return "grand"; }
}
class Other extends Base {
}
echo Child::run(), "\n";
echo GrandChild::run(), "\n";
echo Other::run(), "\n";
echo Child::check(), " ", Other::check(), "\n";
echo Child::withArgs("x", "y"), "\n";
echo GrandChild::withArgs(1, 2), "\n";
echo Other::withArgs("p", "q"), "\n";

abstract class Plain {
    public static function go() { return static::missing(); }
}
class Impl extends Plain {
    protected static function missing() { return "impl"; }
}
class Empty_ extends Plain {}
echo Impl::go(), "\n";
try {
    echo Empty_::go(), "\n";
} catch (Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "child-hook|child\nchild-hook|grand\ncallStatic:hook0|base\nyes no\nx+y@child\n1+2@grand\ncallStatic:combine2\nimpl\nError: Call to undefined method Empty_::missing()\n"
    );
}

/// `empty($o->p)` through `__isset`/`__get` releases the value `__get` returned once it has
/// been tested. A `mixed` return is boxed, and that box used to leak on every call.
#[test]
fn test_empty_through_magic_get_releases_the_read_value() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class M {
    public function __isset($n) { return true; }
    public function __get($n): mixed { return $n === 'a' ? str_repeat('x', 3) : 0; }
}
$m = new M();
$n = 0;
for ($i = 0; $i < 20; $i++) {
    if (empty($m->a)) { $n++; }
    if (empty($m->z)) { $n++; }
}
echo $n, "\n";
"#,
    );
    assert_eq!(out.stdout, "20\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Which magic accessors each probe calls, measured on php 8.5: `isset()` and `empty()` on a
/// class with `__get` but no `__isset` report the name absent WITHOUT calling `__get`, while `??`
/// does call it; with `__isset`, every probe asks it first and only `??`/`empty()` go on to
/// `__get`. The ladder for a `mixed` receiver used to call `__get` for `isset()` and `empty()`,
/// and `??` on a known class skipped `__isset`.
#[test]
fn test_probes_call_the_magic_accessors_php_calls() {
    let out = compile_and_run(
        r#"<?php
class M { public function __get($n) { echo "get($n)\n"; return 1; } }
class N {
    public function __get($n) { echo "nget($n)\n"; return 0; }
    public function __isset($n) { echo "nisset($n)\n"; return true; }
}
$o = new M();
var_dump(isset($o->missing));
var_dump($o->missing ?? "dflt");
var_dump(empty($o->missing));
$n = new N();
var_dump(isset($n->missing));
var_dump($n->missing ?? "dflt");
var_dump(empty($n->missing));
$m = $argc > 0 ? new M() : 5;
var_dump(isset($m->missing));
var_dump($m->missing ?? "dflt");
var_dump(empty($m->missing));
$k = $argc > 0 ? new N() : 5;
var_dump(isset($k->missing));
var_dump($k->missing ?? "dflt");
var_dump(empty($k->missing));
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nget(missing)\nint(1)\nbool(true)\n\
         nisset(missing)\nbool(true)\nnisset(missing)\nnget(missing)\nint(0)\nnisset(missing)\nnget(missing)\nbool(true)\n\
         bool(false)\nget(missing)\nint(1)\nbool(true)\n\
         nisset(missing)\nbool(true)\nnisset(missing)\nnget(missing)\nint(0)\nnisset(missing)\nnget(missing)\nbool(true)\n"
    );
}

/// A borrowed string parameter forwarded to a user function ahead of another argument is pinned
/// while that argument is evaluated. The pin used to be handed to the call as an owned operand,
/// and a callee returning `mixed` (a conservative return-alias summary) then never released it:
/// every `__get` relaying its `$name` leaked one string.
#[test]
fn test_forwarded_borrowed_string_before_another_argument_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function relay(string $name, int $op): mixed { return strlen($name) + $op; }
class W {
    public function __get(string $name): mixed { return relay($name, 0); }
}
function cycle(int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $c = new W(); $sum += $c->label; }
    return $sum;
}
echo cycle(10), "\n";
"#,
    );
    assert_eq!(out.stdout, "50\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies `??` on an overloaded property evaluates its receiver once, before `__isset()` and
/// `__get()`, and releases it: `makeBag()->here ?? $d` calls `makeBag()` once, as PHP does.
#[test]
fn test_coalesce_magic_property_evaluates_its_receiver_once() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Bag {
    public function __isset(string $name): bool { echo "isset;"; return $name === "here"; }
    public function __get(string $name): mixed { echo "get;"; return "v" . strlen($name); }
}
function makeBag(): Bag { echo "make;"; return new Bag(); }
echo makeBag()->here ?? "fallback", "\n";
echo makeBag()->gone ?? "fallback", "\n";
$bags = [new Bag()];
echo $bags[0]->here ?? "fallback", "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "make;isset;get;v4\nmake;isset;fallback\nisset;get;v4\n",
        "stderr: {}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A `mixed` receiver reads through `__get` whatever its declared result: a `string`, `array` or
/// object result moves into the boxed answer with its owner. `isset()` on a class with `__isset`
/// but no `__get` takes `__isset`'s answer. Both used to fall through to the stdClass helper,
/// which warned "Undefined property" and answered null, or reported the name unset.
#[test]
fn test_mixed_receiver_reads_typed_get_results_and_get_less_isset() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public function __get(string $k): string { return "hi-$k"; } }
class D {
    public function __isset(string $k): bool { return true; }
    public function __get(string $k): array { return [$k]; }
}
class E { public function __isset(string $k): bool { return $k === "x"; } }
class F { public function __get(string $k): C { return new C(); } }
function f(mixed $o) { return $o->cfg; }
function g(mixed $o) { return isset($o->x); }
function h(mixed $o) { return isset($o->y); }
var_dump(f(new C));
var_dump(f(new D));
var_dump(f(new F) instanceof C);
var_dump(g(new D), g(new E), h(new E), g(new C));
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(6) \"hi-cfg\"\narray(1) {\n  [0]=>\n  string(3) \"cfg\"\n}\nbool(true)\n\
         bool(true)\nbool(true)\nbool(false)\nbool(false)\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A method call on an UNKNOWN receiver (untyped parameter, array element, closure parameter)
/// reaches `__call()` when the object's class has it but not the method.
///
/// The class-id dispatch ladder behind such a call listed only the classes that declare the
/// method, so `fn ($carry, $e) => $e->getLength()` over Termwind elements died with "Call to
/// undefined method" instead of Element::__call() forwarding to its styles. Argument side effects
/// happen once and in order; an object without `__call()` still gets php's own error.
/// Expected output is real php 8.5 output.
#[test]
fn test_unknown_receiver_method_call_falls_back_to_magic_call() {
    let out = compile_and_run(
        r#"<?php
class A { public function __call($n, $a) { return "magic:$n(" . implode(",", $a) . ")"; } }
class B { public function foo($x = 0) { return "foo$x"; } }
class C extends A { public function bar() { return "bar"; } }
class D { }
function f($o, $x) { return $o->foo($x); }
function g($o) { return $o->bar(); }
echo f(new B, 5), "|", f(new A, 6), "|", f(new C, 7), "|", g(new C), "|", g(new A), "\n";
$items = [new A, new B, new C];
echo implode(",", array_map(fn ($e) => $e->foo(1), $items)), "\n";
echo array_reduce($items, fn ($carry, $e) => $carry . $e->foo(2), ''), "\n";
$n = 0;
function h($o, &$n) { return $o->foo(++$n, ++$n); }
echo h(new A, $n), " n=$n\n";
try { f(new D, 1); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { f(null, 1); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "foo5|magic:foo(6)|magic:foo(7)|bar|magic:bar()\n\
magic:foo(1),foo1,magic:foo(1)\n\
magic:foo(2)foo2magic:foo(2)\n\
magic:foo(1,2) n=2\n\
Error: Call to undefined method D::foo()\n\
Error: Call to a member function foo() on null\n"
    );
}

/// A property read through an UNKNOWN receiver reaches `__get()` and keeps php's access rules.
///
/// The class-id ladder behind `$untyped->name` listed only classes that declare the property, so a
/// class with `__get()` and no such slot fell into the miss arm and warned `Undefined property`.
/// `??` consults `__isset()` first when the class declares one and `__get()` otherwise, `isset()`
/// never calls `__get()`, and a protected slot read from outside the class still raises php's
/// catchable `Error`. The loop is scaled so a per-read leak cannot hide behind live-at-exit values.
/// Expected output is real php 8.5 output.
#[test]
fn test_unknown_receiver_property_read_reaches_magic_get() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Plain { public $name = 'plain'; protected $secret = 's1'; }
class Plain2 { public $name = 'plain2'; }
class Magic { public function __get($n) { return "magic:$n"; } }
class SubMagic extends Magic {}
class Gated {
    public function __isset($n) { return $n === 'yes'; }
    public function __get($n) { return "gated:$n"; }
}
class Hidden {
    private $name = 'hidden';
    public function __get($n) { return "hidden-magic:$n"; }
    public function own($o) { return $o->name; }
}
function nameOf($o) { return $o->name; }
function nameOrDefault($o) { return $o->name ?? 'none'; }
function hasName($o) { return isset($o->name) ? 'y' : 'n'; }
function yesOrDefault($o) { return $o->yes ?? 'none'; }
function secretOf($o) {
    try { return $o->secret; } catch (Error $e) { return get_class($e) . ':' . $e->getMessage(); }
}
$objects = [new Plain, new Plain2, new Magic, new SubMagic, new Gated, new Hidden];
for ($i = 0; $i < 25; $i++) {
    $out = [];
    foreach ($objects as $o) {
        $out[] = nameOf($o) . '/' . nameOrDefault($o) . '/' . hasName($o) . '/' . yesOrDefault($o);
    }
    $out[] = secretOf(new Plain);
    $out[] = (new Hidden)->own(new Hidden);
}
echo implode("\n", $out), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "plain/plain/y/none\n\
plain2/plain2/y/none\n\
magic:name/magic:name/n/magic:yes\n\
magic:name/magic:name/n/magic:yes\n\
gated:name/none/n/gated:yes\n\
hidden-magic:name/hidden-magic:name/n/hidden-magic:yes\n\
Error:Cannot access protected property Plain::$secret\n\
hidden\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
