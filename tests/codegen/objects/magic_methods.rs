//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object magic methods, including `__toString`, `__get`/`__set`, `__call`/`__callStatic`, `__isset`/`__unset`, and `__invoke`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

/// Inaccessible declared names read their magic value while lexical reads keep the real slot.
#[test]
fn test_magic_get_reads_inaccessible_declared_property() {
    let source = r#"<?php
class HiddenMagicRead {
    private mixed $value = 'stored';
    public function __get(string $name): mixed { echo 'get|'; return 'visible'; }
    public function own(): mixed { return $this->value; }
}
function readHiddenMagic(HiddenMagicRead $object): mixed { return $object->value; }
$object = new HiddenMagicRead();
echo readHiddenMagic($object), '|', $object->own();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "get|visible|stored");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

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

/// Verifies a real dynamic entry precedes `__isset()`: a stored value, including a stored
/// `null` and `false`, answers from the instance hash, while only a missing name reaches the
/// accessor. `__get` is never consulted by an `isset()` probe.
#[test]
fn test_magic_isset_prefers_a_real_dynamic_entry() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class HashBag {
    public function __isset($name) { echo "isset:", $name, "|"; return $name === "virtual"; }
    public function __get($name) { echo "get:", $name, "|"; return "value"; }
}
$bag = new HashBag();
$bag->stored = 5;
$bag->storedNull = null;
$bag->storedFalse = false;
echo isset($bag->stored) ? "stored" : "missing", "|";
echo isset($bag->storedNull) ? "stored" : "missing", "|";
echo isset($bag->storedFalse) ? "stored" : "missing", "|";
echo isset($bag->virtual) ? "virtual" : "missing", "|";
echo isset($bag->absent) ? "virtual" : "missing", "\n";
"#,
    );
    assert_eq!(
        out,
        "stored|missing|stored|isset:virtual|virtual|isset:absent|missing\n"
    );
}

/// Verifies a stored dynamic entry still wins on a class with `__get()` but no `__isset()`:
/// `isset()`/`empty()` must probe the hash instead of answering absent before the plan runs,
/// and `__get()` is never called for a probe on such a class.
#[test]
fn test_hash_probe_precedes_absent_probe_on_a_get_only_class() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class GetOnlyBag {
    public function __get($name) { echo "get:", $name, "|"; return 7; }
}
$bag = new GetOnlyBag();
$bag->saved = 3;
echo isset($bag->saved) ? "saved" : "missing", "|";
echo isset($bag->virtual) ? "saved" : "missing", "|";
echo empty($bag->saved) ? "empty" : "set", "|";
echo empty($bag->virtual) ? "empty" : "set", "\n";
"#,
    );
    assert_eq!(out, "saved|missing|set|empty\n");
}

/// Verifies the same dynamic-entry precedence through a by-reference `__isset()` result.
#[test]
fn test_magic_isset_prefers_a_real_dynamic_entry_over_a_reference_accessor() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class ReferenceHashBag {
    public bool $answer = true;
    public function &__isset(string $name): bool { echo "isset:", $name, "|"; return $this->answer; }
}
$bag = new ReferenceHashBag();
$bag->stored = 1;
echo isset($bag->stored) ? "stored" : "missing", "|";
echo isset($bag->virtual) ? "virtual" : "missing", "\n";
"#,
    );
    assert_eq!(out, "stored|isset:virtual|virtual\n");
}

/// Verifies `empty()` on a class with both `__get()` and `__isset()` probes a stored dynamic
/// entry before either accessor: a stored `0` is empty, a stored `5` is not, and only a missing
/// name runs `__isset` (then `__get` when present). A class without `__get()` still answers a
/// true `__isset` as an empty null value.
#[test]
fn test_empty_hash_probe_precedes_magic_accessors() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class EmptyMagicBag {
    public function __isset($name) {
        echo "isset:", $name, "|";
        return $name === "virtual" || $name === "zeroVirtual";
    }
    public function __get($name) {
        echo "get:", $name, "|";
        return $name === "zeroVirtual" ? 0 : 7;
    }
}
$bag = new EmptyMagicBag();
$bag->saved = 0;
$bag->stored = 5;
echo empty($bag->saved) ? "empty" : "set", "|";
echo empty($bag->stored) ? "empty" : "set", "|";
echo empty($bag->virtual) ? "empty" : "set", "|";
echo empty($bag->zeroVirtual) ? "empty" : "set", "|";
echo empty($bag->{'saved'}) ? "empty" : "set", "|";
#[AllowDynamicProperties]
class EmptyIssetOnlyBag {
    public function __isset($name) { echo "only:", $name, "|"; return true; }
}
$only = new EmptyIssetOnlyBag();
$only->saved = 0;
echo empty($only->saved) ? "empty" : "set", "|";
echo empty($only->virtual) ? "empty" : "set", "\n";
"#,
    );
    assert_eq!(
        out,
        "empty|set|isset:virtual|get:virtual|set|isset:zeroVirtual|get:zeroVirtual|empty|empty|empty|only:virtual|empty\n"
    );
}

/// Verifies a stored dynamic entry wins over `__get` on plain reads and `??`, while a miss
/// still calls the accessor; a stored null remains null for a read and selects `??`'s default.
#[test]
fn test_hash_stored_entry_precedes_get_on_reads_and_coalesce() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class ReadHashBag {
    public function __get($name) { echo "get:", $name, "|"; return 1; }
}
$bag = new ReadHashBag();
$bag->stored = 0;
var_dump($bag->stored);
var_dump($bag->stored ?? "default");
var_dump($bag->missing);
var_dump($bag->missing ?? "default");
"#,
    );
    assert_eq!(
        out,
        "int(0)\nint(0)\nget:missing|int(1)\nget:missing|int(1)\n"
    );
}

/// Verifies `empty()`/`isset()` answer a typed uninitialized slot even after the class has
/// reserved a dynamic-property hash.
#[test]
fn test_uninitialized_typed_property_probe_wins_over_dynamic_hash() {
    let out = compile_and_run(
        r#"<?php
class UninitializedWithHash { public int $value; }
$object = new UninitializedWithHash();
$object->dynamic = 1;
var_dump(empty($object->value), isset($object->value));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\n");
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

/// Verifies `??` checks a stored dynamic hash entry before `__isset()`, so a saved `0` answers
/// from storage while a missing name still consults the accessor and falls back.
#[test]
fn test_coalesce_prefers_a_stored_dynamic_entry_over_isset() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class CoalesceHashBag {
    public function __isset($n) { echo "isset(" . $n . ")|"; return false; }
    public function __get($n) { echo "get(" . $n . ")|"; return 1; }
}
$bag = new CoalesceHashBag;
$bag->saved = 0;
var_dump($bag->saved ?? 9);
var_dump($bag->missing ?? 9);
"#,
    );
    assert_eq!(out, "int(0)\nisset(missing)|int(9)\n");
}

/// Verifies `isset()`/`empty()` convert an unannotated `__isset()` return value with PHP
/// truthiness, including the string `"0"`, empty strings, and empty arrays, and release the
/// boxed accessor result.
#[test]
fn test_magic_isset_result_uses_php_truthiness_and_releases_it() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class TruthyIssetBag {
    public function __isset($name) {
        return match ($name) {
            "zero" => "0",
            "emptyString" => "",
            "emptyArray" => [],
            "fullArray" => [1],
            default => 5,
        };
    }
    public function __get($name) { return "value"; }
}
$bag = new TruthyIssetBag;
var_dump(isset($bag->zero), isset($bag->emptyString), isset($bag->emptyArray), isset($bag->fullArray), isset($bag->int));
var_dump(empty($bag->zero), empty($bag->emptyString), empty($bag->emptyArray), empty($bag->fullArray));
var_dump($bag->zero ?? "d", $bag->emptyArray ?? "d", $bag->fullArray ?? "d");
"#,
    );
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nbool(false)\nbool(true)\nbool(true)\n\
         bool(true)\nbool(true)\nbool(true)\nbool(false)\n\
         string(1) \"d\"\nstring(1) \"d\"\nstring(5) \"value\"\n",
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a stored dynamic property keeps its assigned type even when the class declares a
/// typed `__get()`: only a missing entry answers the accessor, and its declared return type is
/// never applied to the stored value.
#[test]
fn test_dynamic_property_read_bypasses_typed_magic_get_return() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class DynMagicBag {
    public function __get($name): int { echo "get:$name|"; return 7; }
}
$bag = new DynMagicBag();
$bag->saved = "text";
var_dump($bag->saved);
$bag->count = 42;
var_dump($bag->count);
var_dump($bag->missing);
"#,
    );
    assert_eq!(out, "string(4) \"text\"\nint(42)\nget:missing|int(7)\n");
}

/// Verifies a runtime-name read probes the receiver's dynamic-property hash before the
/// accessor fallback: a name that only a used subclass declares still enters the candidate
/// ladder, and the base class's stored entry must win over the deferred `__get`.
#[test]
fn test_runtime_name_read_probes_dynamic_storage_before_magic_fallback() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class RuntimeDynBag {
    public function __get($name) { return "magic:$name"; }
}
class RuntimeDynBagChild extends RuntimeDynBag {
    public string $saved = "declared";
}
$bag = new RuntimeDynBag();
$bag->saved = 'text';
$bag->other = 42;
$name = 'saved';
var_dump($bag->{$name});
$name = 'other';
var_dump($bag->{$name});
$child = new RuntimeDynBagChild();
$name = 'saved';
var_dump($child->{$name});
"#,
    );
    assert_eq!(out, "string(4) \"text\"\nint(42)\nstring(8) \"declared\"\n");
}
