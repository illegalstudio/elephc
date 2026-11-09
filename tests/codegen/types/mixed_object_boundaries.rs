//! Purpose:
//! Regression coverage for issue #2309: a `mixed` or `object|null` value reaching a return,
//! parameter or typed property declared as a class, interface, `object` or nullable/union of
//! those compiles, and PHP's runtime check throws its exact `TypeError`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every fixture mixes values that pass with values that fail, so both the accepted edge
//!   (the object arrives intact) and the rejected edge (PHP's message) are asserted.
//! - Argument messages end in `called in <file> on line N`; the fixtures keep the line and
//!   drop the temporary path, so `ELEPHC_PHP_CHECK=1` compares them with php byte for byte.

use super::*;

/// Issue #2309 repro: an array element read (`mixed`) returned from `?Timer`, as revolt's
/// `TimerQueue::extract()` does.
#[test]
fn test_mixed_array_element_returns_through_a_nullable_class_return() {
    let out = compile_and_run(
        r#"<?php
final class Timer { public function __construct(public readonly string $id) {} }

final class TimerQueue {
    /** @var array<int, Timer> */
    private array $timers = [];
    public function add(Timer $t): void { $this->timers[] = $t; }
    public function extract(): ?Timer {
        $t = $this->timers[0] ?? null;
        return $t;
    }
}
$q = new TimerQueue(); $q->add(new Timer('a'));
echo $q->extract()?->id, "\n";
"#,
    );
    assert_eq!(out, r#"a
"#);
}

/// A `mixed` value returned from class, interface, `object`, nullable, union, `static` and
/// `self` declarations: matching values pass, others throw PHP's `Return value` `TypeError`.
#[test]
fn test_mixed_reaches_class_like_returns_with_runtime_type_errors() {
    let out = compile_and_run(
        r#"<?php
interface Shape { public function name(): string; }
final class Sq implements Shape { public function name(): string { return "sq"; } }
final class Tri implements Shape { public function name(): string { return "tri"; } }
class Base {
    public static function make(mixed $m): static { return $m; }
    public function me(mixed $m): self { return $m; }
}
final class Child extends Base {}
final class R {
    public static function cls(mixed $m): Sq { return $m; }
    public static function ifc(mixed $m): Shape { return $m; }
    public static function obj(mixed $m): object { return $m; }
    public static function nul(mixed $m): ?Sq { return $m; }
    public static function uni(mixed $m): Sq|Tri|null { return $m; }
}
function show(mixed $v): string { return $v === null ? "null" : get_class($v); }
$cases = [new Sq(), new Tri(), new Child(), new Base(), 7, null];
foreach ($cases as $m) {
    try { echo show(R::cls($m)), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { echo show(R::ifc($m)), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { echo show(R::obj($m)), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { echo show(R::nul($m)), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { echo show(R::uni($m)), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { echo show(Child::make($m)), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { echo show((new Child())->me($m)), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(out, r#"Sq
Sq
Sq
Sq
Sq
Base::make(): Return value must be of type Child, Sq returned
Base::me(): Return value must be of type Base, Sq returned
R::cls(): Return value must be of type Sq, Tri returned
Tri
Tri
R::nul(): Return value must be of type ?Sq, Tri returned
Tri
Base::make(): Return value must be of type Child, Tri returned
Base::me(): Return value must be of type Base, Tri returned
R::cls(): Return value must be of type Sq, Child returned
R::ifc(): Return value must be of type Shape, Child returned
Child
R::nul(): Return value must be of type ?Sq, Child returned
R::uni(): Return value must be of type Sq|Tri|null, Child returned
Child
Child
R::cls(): Return value must be of type Sq, Base returned
R::ifc(): Return value must be of type Shape, Base returned
Base
R::nul(): Return value must be of type ?Sq, Base returned
R::uni(): Return value must be of type Sq|Tri|null, Base returned
Base::make(): Return value must be of type Child, Base returned
Base
R::cls(): Return value must be of type Sq, int returned
R::ifc(): Return value must be of type Shape, int returned
R::obj(): Return value must be of type object, int returned
R::nul(): Return value must be of type ?Sq, int returned
R::uni(): Return value must be of type Sq|Tri|null, int returned
Base::make(): Return value must be of type Child, int returned
Base::me(): Return value must be of type Base, int returned
R::cls(): Return value must be of type Sq, null returned
R::ifc(): Return value must be of type Shape, null returned
R::obj(): Return value must be of type object, null returned
null
null
Base::make(): Return value must be of type Child, null returned
Base::me(): Return value must be of type Base, null returned
"#);
}

/// A `mixed` argument bound to class, interface, `object`, nullable, union and promoted
/// constructor parameters, from a local and from a call result that cannot be re-read.
#[test]
fn test_mixed_reaches_class_like_parameters_with_runtime_type_errors() {
    let out = compile_and_run(
        r#"<?php
interface Shape { public function name(): string; }
final class Sq implements Shape { public function name(): string { return "sq"; } }
final class Tri implements Shape { public function name(): string { return "tri"; } }
final class Holder { public function __construct(public ?Shape $shape) {} }
function msg(TypeError $e): string {
    $text = $e->getMessage();
    $at = strpos($text, ', called in ');
    return $at === false ? $text : substr($text, 0, $at) . " @" . substr($text, strrpos($text, " ") + 1);
}
function cls(Sq $s): string { return $s->name(); }
function ifc(Shape $s): string { return $s->name(); }
function obj(object $o): string { return get_class($o); }
function nul(?Sq $s): string { return $s === null ? "null" : $s->name(); }
function uni(Sq|Tri|null $s): string { return $s === null ? "null" : $s->name(); }
function pick(int $i): mixed { $all = [new Sq(), new Tri(), 7, null]; return $all[$i]; }
foreach ([0, 1, 2, 3] as $i) {
    $m = pick($i);
    try { echo cls(pick($i)), "\n"; } catch (TypeError $e) { echo msg($e), "\n"; }
    try { echo ifc($m), "\n"; } catch (TypeError $e) { echo msg($e), "\n"; }
    try { echo obj(pick($i)), "\n"; } catch (TypeError $e) { echo msg($e), "\n"; }
    try { echo nul($m), "\n"; } catch (TypeError $e) { echo msg($e), "\n"; }
    try { echo uni(pick($i)), "\n"; } catch (TypeError $e) { echo msg($e), "\n"; }
    try { $h = new Holder($m); echo $h->shape?->name() ?? "null", "\n"; } catch (TypeError $e) { echo msg($e), "\n"; }
}
"#,
    );
    assert_eq!(out, r#"sq
sq
Sq
sq
sq
sq
cls(): Argument #1 ($s) must be of type Sq, Tri given @19
tri
Tri
nul(): Argument #1 ($s) must be of type ?Sq, Tri given @22
tri
tri
cls(): Argument #1 ($s) must be of type Sq, int given @19
ifc(): Argument #1 ($s) must be of type Shape, int given @20
obj(): Argument #1 ($o) must be of type object, int given @21
nul(): Argument #1 ($s) must be of type ?Sq, int given @22
uni(): Argument #1 ($s) must be of type Sq|Tri|null, int given @23
Holder::__construct(): Argument #1 ($shape) must be of type ?Shape, int given @24
cls(): Argument #1 ($s) must be of type Sq, null given @19
ifc(): Argument #1 ($s) must be of type Shape, null given @20
obj(): Argument #1 ($o) must be of type object, null given @21
null
null
null
"#);
}

/// An `object|null` value reaching `?Class` properties, parameters and returns (revolt's
/// `DriverSuspension::$suspendedFiber`), plus a `mixed` value stored to a `Class` property.
#[test]
fn test_object_or_null_reaches_narrower_class_boundaries() {
    let out = compile_and_run(
        r#"<?php
final class Fiberish { public function hi(): string { return "hi"; } }
final class Suspension {
    public ?Fiberish $suspended = null;
    public Fiberish $current;
    public function park(object|null $fiber): void { $this->suspended = $fiber; }
    public function own(mixed $fiber): void { $this->current = $fiber; }
    public function back(object|null $fiber): ?Fiberish { return $fiber; }
}
function take(?Fiberish $f): string { return $f === null ? "null" : $f->hi(); }
function relay(object|null $o): string { return take($o); }
function msg(TypeError $e): string {
    $text = $e->getMessage();
    $at = strpos($text, ', called in ');
    return $at === false ? $text : substr($text, 0, $at) . " @" . substr($text, strrpos($text, " ") + 1);
}
$s = new Suspension();
foreach ([new Fiberish(), new ArrayObject([]), null] as $o) {
    try { $s->park($o); echo $s->suspended?->hi() ?? "null", "\n"; } catch (TypeError $e) { echo msg($e), "\n"; }
    try { $s->own($o); echo $s->current->hi(), "\n"; } catch (TypeError $e) { echo msg($e), "\n"; }
    try { echo $s->back($o)?->hi() ?? "null", "\n"; } catch (TypeError $e) { echo msg($e), "\n"; }
    try { echo relay($o), "\n"; } catch (TypeError $e) { echo msg($e), "\n"; }
}
"#,
    );
    assert_eq!(out, r#"hi
hi
hi
hi
Cannot assign ArrayObject to property Suspension::$suspended of type ?Fiberish
Cannot assign ArrayObject to property Suspension::$current of type Fiberish
Suspension::back(): Return value must be of type ?Fiberish, ArrayObject returned
take(): Argument #1 ($f) must be of type ?Fiberish, ArrayObject given @11
null
Cannot assign null to property Suspension::$current of type Fiberish
null
null
"#);
}

/// A WeakMap lookup returned from `?Val`, and return-boundary `TypeError`s caught around a
/// callee with no other throw and unwound through `finally`.
#[test]
fn test_mixed_class_returns_from_weak_map_and_through_finally() {
    let out = compile_and_run(
        r#"<?php
final class Key {}
final class Val { public function __construct(public string $v) {} }
final class Store {
    private WeakMap $map;
    public function __construct() { $this->map = new WeakMap(); }
    public function put(Key $k, mixed $v): void { $this->map[$k] = $v; }
    public function get(Key $k): ?Val { return isset($this->map[$k]) ? $this->map[$k] : null; }
}
function unchecked(mixed $m): Key { return $m; }
function guarded(mixed $m): Key { try { return $m; } finally { echo "finally|"; } }
$s = new Store(); $hit = new Key(); $bad = new Key();
$s->put($hit, new Val("found"));
$s->put($bad, "text");
echo $s->get($hit)?->v, "|", var_export($s->get(new Key()), true), "\n";
try { $s->get($bad); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
foreach ([new Key(), 1.5] as $m) {
    try { unchecked($m); echo "ok\n"; } catch (TypeError $e) { echo "caught: ", $e->getMessage(), "\n"; }
    try { guarded($m); echo "ok\n"; } catch (TypeError $e) { echo "caught: ", $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(out, r#"found|NULL
Store::get(): Return value must be of type ?Val, string returned
ok
finally|ok
caught: unchecked(): Return value must be of type Key, float returned
finally|caught: guarded(): Return value must be of type Key, float returned
"#);
}
