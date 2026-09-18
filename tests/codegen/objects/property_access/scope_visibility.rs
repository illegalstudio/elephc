//! Purpose:
//! Integration or regression tests for php's SCOPE-dependent answer to one property name, on a
//! READ and on a MUTATION: the value-read refusal, the silent `isset()` / `empty()` / `??` probe,
//! and the distinct dynamic property a strict ancestor's private name resolves to outside the
//! class that declared it, including what a write, a compound write, an increment, an `unset()`
//! and a magic accessor do with that name.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.
//! - Every expectation below was measured against php 8.5.10, including the verbatim `Error`
//!   wording, which carries no scope suffix.
//! - The mutation fixtures pin the exact COUNT of php's `Creation of dynamic property` notices and
//!   the RUNTIME class each one names, because both are decided per runtime class: a receiver
//!   typed as a base class holding a subclass instance reports the subclass.

use super::*;

/// Verifies a runtime-name value READ php refuses raises the catchable `Error` instead of
/// reading the slot, while `isset()` answers false without raising.
///
/// `isset($o->{$k})` lowers through the same dynamic read as `$o->{$k}`, so before the fetch mode
/// travelled on the instruction the two could not disagree: refusing the read made `isset()`
/// throw, and accepting it let global scope read a private slot by name. Both readers below prove
/// the slot still holds its default.
#[test]
fn test_runtime_name_read_of_an_inaccessible_property_raises_but_isset_answers_false() {
    let out = compile_and_run(
        r#"<?php
class D { private int $n = 7; public function readN(): int { return $this->n; } }
class Prot { protected int $p = 3; public function readP(): int { return $this->p; } }
class Outsider {
    public function poke(Prot $o, string $key): void {
        var_dump(isset($o->{$key}));
        try { $v = $o->{$key}; echo "no throw:" . $v . ";"; }
        catch (Error $e) { echo $e->getMessage() . ";"; }
    }
}
$d = new D();
$key = "n";
var_dump(isset($d->{$key}));
try { $v = $d->{$key}; echo "no throw:" . $v . ";"; }
catch (Error $e) { echo $e->getMessage() . ";"; }
echo $d->readN() . ";";
$p = new Prot();
(new Outsider())->poke($p, "p");
echo $p->readP();
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nCannot access private property D::$n;7;bool(false)\n\
         Cannot access protected property Prot::$p;3"
    );
}

/// Verifies `empty()` and `??` are silent probes for a property php refuses, exactly like
/// `isset()`.
///
/// php answers `true` and the default without raising. Wiring only `isset()` to the probe fetch
/// mode would have made these two throw where php is silent.
#[test]
fn test_empty_and_coalesce_probe_an_inaccessible_property_without_raising() {
    let out = compile_and_run(
        r#"<?php
class D { private int $n = 7; public function readN(): int { return $this->n; } }
$d = new D();
$key = "n";
var_dump(empty($d->{$key}));
var_dump($d->{$key} ?? "dflt");
echo $d->readN();
"#,
    );
    assert_eq!(out, "bool(true)\nstring(4) \"dflt\"\n7");
}

/// Verifies a strict ancestor's PRIVATE name is a DYNAMIC property from the child scope and from
/// global scope, by runtime name and by plain name alike.
///
/// php 7.4 removed shadow properties: `Base::$n` lives under a mangled key, so `B`'s by-name table
/// does not contain it at all. A read warns `Undefined property: B::$n` and answers null, `isset()`
/// answers false in silence, and the ancestor's own scope still reads its slot. The plain name used
/// to be a compile error here, which is php's answer for a private property declared by the
/// receiver's OWN class, not for an inherited one.
#[test]
fn test_strict_ancestor_private_name_reads_as_a_dynamic_property() {
    let out = compile_and_run_capture(
        r#"<?php
class A { private int $n = 1; public function readA(): int { return $this->n; } }
class B extends A {
    public function childRuntime(string $key): void {
        var_dump(isset($this->{$key}));
        var_dump($this->{$key});
    }
    public function childDirect(): void {
        var_dump(isset($this->n));
        var_dump($this->n);
    }
}
$b = new B();
$b->childRuntime("n");
$b->childDirect();
$key = "n";
var_dump(isset($b->{$key}));
var_dump($b->{$key});
var_dump(isset($b->n));
var_dump($b->n);
echo $b->readA();
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nNULL\nbool(false)\nNULL\nbool(false)\nNULL\nbool(false)\nNULL\n1"
    );
    assert_eq!(
        out.stderr.matches("Warning: Undefined property: B::$n").count(),
        4,
        "each VALUE read warns and each probe stays silent: {}",
        out.stderr
    );
}

/// Verifies a dynamic property created under a strict ancestor's private name is what the child
/// and global scopes then read, while the ancestor keeps its own slot.
///
/// The class opts into dynamic properties so the entry has somewhere to live. php prints
/// `int(42)` for every reader below except `A::readA()`, which still answers `1`.
#[test]
fn test_dynamic_entry_under_an_ancestor_private_name_is_read_back() {
    let out = compile_and_run_capture(
        r#"<?php
#[\AllowDynamicProperties]
class A { private int $n = 1; public function readA(): int { return $this->n; } }
class B extends A {}
$b = new B();
$key = "n";
var_dump(isset($b->{$key}));
$b->{$key} = 42;
var_dump(isset($b->{$key}));
var_dump($b->{$key});
var_dump(isset($b->n));
var_dump($b->n);
echo $b->readA();
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(true)\nint(42)\nbool(true)\nint(42)\n1"
    );
    assert!(
        !out.stderr.contains("Undefined property"),
        "a present dynamic entry must not warn: {}",
        out.stderr
    );
}

/// Verifies a boxed `Mixed` receiver cannot read private storage by plain name from an unrelated
/// scope, and that its probes stay silent.
///
/// The `Mixed` ladder dispatches on the receiver's runtime class id, so it reached the declared
/// slot with no visibility check at all. That was the last remaining plain-name route into a
/// private slot from global scope.
#[test]
fn test_mixed_receiver_read_of_a_private_property_raises_but_probes_stay_silent() {
    let out = compile_and_run(
        r#"<?php
class Sec { private int $s = 5; public int $open = 1; public function readS(): int { return $this->s; } }
function pick(bool $flag): mixed { return $flag ? new Sec() : 1; }
$m = pick(true);
try { $v = $m->s; echo "no throw:" . $v . ";"; }
catch (Error $e) { echo $e->getMessage() . ";"; }
var_dump(isset($m->s));
var_dump(empty($m->s));
var_dump($m->s ?? "dflt");
var_dump($m->open);
echo (new Sec())->readS();
"#,
    );
    assert_eq!(
        out,
        "Cannot access private property Sec::$s;bool(false)\nbool(true)\n\
         string(4) \"dflt\"\nint(1)\n5"
    );
}

/// Verifies the scope-aware read leaves php's other property answers exactly as they were.
///
/// Two same-named private slots stay apart, a protected property stays reachable down the
/// hierarchy, stdClass and `__get` / `__isset` keep their own dispatch, a get-hooked property
/// still runs its accessor, and a nullsafe read on null still answers null.
#[test]
fn test_scope_aware_reads_preserve_shadowing_hierarchy_magic_and_hooks() {
    let out = compile_and_run(
        r#"<?php
class Base { private int $p = 10; public function readP(): int { return $this->p; } public function readDyn(string $k): int { return $this->{$k}; } }
class Child extends Base { private int $p = 20; public function childP(): int { return $this->p; } public function childDyn(string $k): int { return $this->{$k}; } }
class Par { public int $a = 1; protected int $b = 2; public function readB(): int { return $this->b; } }
class Kid extends Par { public function kidDyn(string $k): int { return $this->{$k}; } }
class Magic {
    private array $bag = ["z" => 9];
    public function __get($n) { return $this->bag[$n] ?? "none"; }
    public function __isset($n) { return isset($this->bag[$n]); }
}
class Hooked { public int $h = 4 { get => $this->h * 2; } }
$c = new Child();
echo $c->readP() . ";" . $c->childP() . ";" . $c->readDyn("p") . ";" . $c->childDyn("p") . ";";
$k = new Kid();
echo $k->a . ";" . $k->readB() . ";" . $k->kidDyn("b") . ";";
$s = new stdClass();
$s->x = 7;
$name = "x";
echo $s->{$name} . ";" . var_export(isset($s->{$name}), true) . ";";
$mg = new Magic();
echo $mg->z . ";" . var_export(isset($mg->z), true) . ";" . var_export(isset($mg->q), true) . ";" . $mg->q . ";";
echo (new Hooked())->h . ";";
$n = null;
var_dump($n?->whatever);
"#,
    );
    assert_eq!(
        out,
        "10;20;10;20;1;2;2;7;true;9;true;false;none;8;NULL\n"
    );
}

/// Verifies `??=` reads its target through the same silent probe, then obeys php's WRITE answer.
///
/// `??=` exists so the target may be absent, so the read half must never raise. php then applies
/// the ordinary write rules: a strict ancestor's private name creates a dynamic property, while a
/// private property declared by the receiver's own class refuses the STORE. Measured against php
/// 8.5.10, which prints exactly the same sequence.
#[test]
fn test_coalesce_assign_probes_the_target_then_obeys_the_write_answer() {
    let out = compile_and_run(
        r#"<?php
class A { private int $m = 1; public function readA(): int { return $this->m; } }
#[\AllowDynamicProperties]
class B extends A {}
class Sec { private int $s = 5; public function readS(): int { return $this->s; } }
function pick(bool $flag): mixed { return $flag ? new Sec() : 1; }
$b = new B();
$key = "m";
$b->{$key} ??= 55;
var_dump($b->{$key}, $b->m, $b->readA());
$b->{$key} ??= 99;
var_dump($b->{$key});
$m = pick(true);
try { $m->s ??= "mx"; var_dump($m->s); }
catch (Error $e) { echo "coalesce assign:" . $e->getMessage() . ";"; }
echo (new Sec())->readS();
"#,
    );
    assert_eq!(
        out,
        "int(55)\nint(55)\nint(1)\nint(55)\ncoalesce assign:Cannot access private property Sec::$s;5"
    );
}

/// Verifies `$o?->{$k}` carries the probe fetch mode through the nullsafe CHAIN lowering.
///
/// A `?->` operand is flattened into a chain before the operand-shaped probe routes ever see it,
/// so `isset()`, `empty()` and `??` reached the chain's ordinary value read and `empty($d?->{$k})`
/// raised where php answers `true`. A null receiver still short-circuits to null without a
/// property-on-null warning, and the plain value read still raises.
#[test]
fn test_nullsafe_runtime_name_probes_stay_silent_through_the_chain() {
    let out = compile_and_run_capture(
        r#"<?php
class D { private int $n = 7; public function readN(): int { return $this->n; } }
function pick(bool $flag): ?D { return $flag ? new D() : null; }
$d = pick(true);
$z = pick(false);
$key = "n";
var_dump(isset($d?->{$key}));
var_dump(empty($d?->{$key}));
var_dump($d?->{$key} ?? "dflt");
var_dump(isset($z?->{$key}));
var_dump(empty($z?->{$key}));
var_dump($z?->{$key} ?? "dflt");
try { var_dump($d?->{$key}); }
catch (Error $e) { echo "read:" . $e->getMessage() . "\n"; }
var_dump($z?->{$key});
echo $d->readN();
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(true)\nstring(4) \"dflt\"\nbool(false)\nbool(true)\n\
         string(4) \"dflt\"\nread:Cannot access private property D::$n\nNULL\n7"
    );
    assert!(
        !out.stderr.contains("Attempt to read property"),
        "a nullsafe hop must not warn on its null receiver: {}",
        out.stderr
    );
}

/// Verifies a boxed `Mixed` receiver warns `Undefined property` for a strict ancestor's private
/// name on a value READ, and stays silent for all three probes.
///
/// The `Mixed` runtime-name ladder dispatches on the receiver's class id AND the name, so a
/// scope-dynamic name gets its own arm there. Dropping it instead sent the name to the shared miss
/// arm, which answers `null` without a diagnostic, so php's warning went missing on that one path
/// while the typed-receiver path reported it.
#[test]
fn test_mixed_receiver_runtime_name_ancestor_private_warns_only_on_the_value_read() {
    let out = compile_and_run_capture(
        r#"<?php
class A { private int $n = 1; public function readA(): int { return $this->n; } }
class B extends A {}
function pick(bool $flag): mixed { return $flag ? new B() : 1; }
$m = pick(true);
$key = "n";
var_dump(isset($m->{$key}));
var_dump($m->{$key});
var_dump(empty($m->{$key}));
var_dump($m->{$key} ?? "dflt");
echo (new B())->readA();
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nNULL\nbool(true)\nstring(4) \"dflt\"\n1"
    );
    assert_eq!(
        out.stderr.matches("Warning: Undefined property: B::$n").count(),
        1,
        "only the VALUE read warns: {}",
        out.stderr
    );
}

/// Verifies an exception thrown from `__isset` propagates out of `isset()` and `empty()`.
///
/// php's probes suppress its own access and miss diagnostics, not user code: `__isset`, `__get` and
/// property hooks all still run and may throw. This is why `PropertyFetchMode` is a diagnostic
/// selector and NOT an effect narrowing, and why `Op::DynamicPropGet` keeps its conservative
/// `may_throw` / `may_warn` contract in both modes.
#[test]
fn test_exception_from_magic_isset_propagates_out_of_the_probe() {
    let out = compile_and_run(
        r#"<?php
class M {
    public function __isset($name) { throw new Exception("boom " . $name); }
    public function __get($name) { return "g" . $name; }
}
$m = new M();
try { var_dump(isset($m->zz)); } catch (Exception $e) { echo "isset:" . $e->getMessage() . ";"; }
try { var_dump(empty($m->zz)); } catch (Exception $e) { echo "empty:" . $e->getMessage() . ";"; }
echo "done";
"#,
    );
    assert_eq!(out, "isset:boom zz;empty:boom zz;done");
}

/// Verifies a class that declares a magic accessor gets php's answer withheld, never its storage.
///
/// php consults `__get` and `__isset` BEFORE it reports anything: on a class that declares them a
/// private property reached from an unrelated scope answers the accessor, never
/// `Cannot access private property`, and a name php resolves to a dynamic property answers the
/// accessor rather than warning `Undefined property`. This compiler cannot dispatch an accessor
/// for a RUNTIME property name yet, so a read of such a name has three wrong answers available and
/// exactly one safe one:
///   - raising or warning would invent a diagnostic php never reports;
///   - reading the declared slot would hand an unrelated scope the private storage php is hiding;
///   - php `null` is wrong in VALUE only, and exposes nothing.
/// `PropertyNameArm::MagicDeferred` takes the third. The php-correct value (`magic:secret` and
/// `inherited:n`, measured on php 8.5.10) arrives with the dedicated runtime-name magic dispatch
/// phase, which is when this test's expectations change.
///
/// The two declaring scopes below prove the storage is intact and still readable from inside.
#[test]
fn test_magic_accessor_class_withholds_its_answer_without_exposing_the_slot() {
    let out = compile_and_run_capture(
        r#"<?php
class M {
    private int $secret = 5;
    public function __isset($name) { return $name === "secret"; }
    public function __get($name) { return "magic:" . $name; }
    public function readSecret(): int { return $this->secret; }
}
class A { private int $n = 1; public function readA(): int { return $this->n; } }
class B extends A { public function __get($name) { return "inherited:" . $name; } }
$m = new M();
$key = "secret";
try { var_dump($m->{$key}); } catch (Error $e) { echo "ERR:" . $e->getMessage() . ";"; }
var_dump(isset($m->{$key}));
$b = new B();
$name = "n";
var_dump($b->{$name});
echo "inside:" . $m->readSecret() . ":" . $b->readA();
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    // The private payloads are 5 and 1. Neither may appear in an out-of-scope runtime-name read.
    assert_eq!(
        out.stdout,
        "NULL\nbool(false)\nNULL\ninside:5:1",
        "out-of-scope runtime reads must answer NULL and the declaring methods must still read \
         their own slots"
    );
    assert!(
        !out.stdout.contains("int(5)") && !out.stdout.contains("int(1)"),
        "a private payload must never reach an out-of-scope runtime-name read: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("ERR:"),
        "php answers the accessor here, so the access error must not be raised: {}",
        out.stdout
    );
    assert!(
        !out.stderr.contains("Undefined property"),
        "php answers the accessor here, so the undefined-property warning must not be emitted: {}",
        out.stderr
    );
}

/// Counts php's dynamic-property creation notices for one class in a fixture's stderr.
fn dynamic_property_notices(stderr: &str, class_name: &str) -> usize {
    let needle = format!("Creation of dynamic property {}::$", class_name);
    stderr.matches(needle.as_str()).count()
}

/// Verifies an ordinary class stores and reads a missing property whose name exists only at run
/// time, and reports the PHP 8.5 creation deprecation exactly once.
///
/// This uses the plain assignment path directly. No clone override reserves storage as a side
/// effect or masks a missing runtime-name reservation.
#[test]
fn test_runtime_built_missing_name_round_trips_on_an_ordinary_class() {
    let out = compile_and_run_capture(
        r#"<?php
class RuntimePlain {
    public string $declared = 'slot';
}
function runtimeName(): string { return chr(100) . chr(121) . chr(110); }
function put(RuntimePlain $object, string $name, mixed $value): void {
    $object->{$name} = $value;
}
function get(RuntimePlain $object, string $name): mixed {
    return $object->{$name};
}
$object = new RuntimePlain();
$name = runtimeName();
put($object, $name, 'stored');
echo get($object, $name), ':', $object->declared;
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "stored:slot");
    assert_eq!(
        dynamic_property_notices(&out.stderr, "RuntimePlain"),
        1,
        "{}",
        out.stderr
    );
}

/// Verifies runtime-name misses call `__set` for typed and Mixed receivers.
///
/// The recursion guard is keyed by receiver plus property name. Inside `__set('outer')`, writing
/// `inner` must invoke a nested `__set('inner')`, while routing either active name through a
/// helper must suppress only the matching reentry and create that dynamic property.
#[test]
fn test_runtime_built_missing_name_dispatches_set_with_pair_specific_reentry() {
    let out = compile_and_run_capture(
        r#"<?php
class RuntimeMagic {
    public function __set(string $name, mixed $value): void {
        echo "magic:$name=$value;";
    }
}
class RuntimeSelfStore {
    public function store(string $name, mixed $value): void {
        $this->{$name} = $value;
    }
    public function __set(string $name, mixed $value): void {
        echo "self:$name;";
        if ($name === 'outer') {
            $inner = chr(105) . chr(110) . chr(110) . chr(101) . chr(114);
            $this->{$inner} = 'nested';
        }
        $this->store($name, $value);
    }
}
function runtimeMagicName(): string { return chr(102) . chr(114) . chr(101) . chr(115) . chr(104); }
function typedPut(RuntimeMagic $object, string $name): void { $object->{$name} = 'typed'; }
function mixedMagic(): mixed { return new RuntimeMagic(); }
function mixedPut(mixed $object, string $name): void { $object->{$name} = 'mixed'; }
$name = runtimeMagicName();
typedPut(new RuntimeMagic(), $name);
mixedPut(mixedMagic(), $name);
$stored = new RuntimeSelfStore();
$outer = chr(111) . chr(117) . chr(116) . chr(101) . chr(114);
$inner = chr(105) . chr(110) . chr(110) . chr(101) . chr(114);
$stored->{$outer} = 'value';
echo $stored->{$outer}, ':', $stored->{$inner};
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "magic:fresh=typed;magic:fresh=mixed;self:outer;self:inner;value:nested"
    );
    assert_eq!(
        dynamic_property_notices(&out.stderr, "RuntimeMagic"),
        0,
        "{}",
        out.stderr
    );
    assert_eq!(
        dynamic_property_notices(&out.stderr, "RuntimeSelfStore"),
        2,
        "{}",
        out.stderr
    );
}

/// Verifies literal-name writes use the receiver/name guard too: the same name stores directly,
/// while a different literal name performs a nested `__set` dispatch.
#[test]
fn test_direct_name_setter_reentry_is_pair_specific() {
    let out = compile_and_run_capture(
        r#"<?php
class DirectLiteralSet {
    public int $calls = 0;
    public function __set(string $name, mixed $value): void {
        $this->calls++;
        echo "set:$name;";
        if ($name === "outer") {
            $this->inner = "nested";
            $this->outer = $value;
            return;
        }
        $this->inner = $value;
    }
}
$object = new DirectLiteralSet();
$object->outer = "first";
$object->outer = "second";
echo $object->calls, ":", $object->outer, ":", $object->inner;
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "set:outer;set:inner;2:second:nested");
    assert_eq!(
        dynamic_property_notices(&out.stderr, "DirectLiteralSet"),
        2,
        "{}",
        out.stderr
    );
}

/// Verifies a literal-name `__set` selected by a runtime subclass guard shares the same reentry
/// suppression and existing-entry probe as a setter selected from the static receiver class.
#[test]
fn test_direct_name_subclass_setter_reentry_uses_runtime_guard() {
    let out = compile_and_run_capture(
        r#"<?php
class DirectSetAncestor { private mixed $stored = null; }
class DirectSetBase extends DirectSetAncestor {}
class DirectSetChild extends DirectSetBase {
    public int $calls = 0;
    public function __set(string $name, mixed $value): void {
        $this->calls++;
        $this->stored = $value;
    }
}
function direct_set_through_base(DirectSetBase $object, mixed $value): void {
    $object->stored = $value;
}
$object = new DirectSetChild();
direct_set_through_base($object, "first");
direct_set_through_base($object, "second");
echo $object->calls, ":", $object->stored;
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "1:second");
    assert_eq!(
        dynamic_property_notices(&out.stderr, "DirectSetChild"),
        1,
        "{}",
        out.stderr
    );
}

/// Verifies a magic-set guard suspended on a Fiber stack is detached from the main context.
/// Destroying that Fiber unmaps its stack, so the next runtime-name write must not scan the
/// retired guard node before dispatching its own `__set` call.
#[test]
fn test_runtime_set_guard_does_not_retain_a_destroyed_suspended_fiber_stack() {
    let out = compile_and_run_capture(
        r#"<?php
class RuntimeFiberGuard {
    public function store(string $name, mixed $value): void {
        $this->{$name} = $value;
    }
    public function __set(string $name, mixed $value): void {
        echo "set:$name;";
        if ($name === 'park') {
            Fiber::suspend();
        }
        $this->store($name, $value);
    }
}
function runtimeFiberParkName(): string { return chr(112) . 'ark'; }
function runtimeFiberNextName(): string { return chr(110) . 'ext'; }
$fiber = new Fiber(function(): void {
    $held = new RuntimeFiberGuard();
    $name = runtimeFiberParkName();
    $held->{$name} = 'held';
});
$fiber->start();
unset($fiber);
$live = new RuntimeFiberGuard();
$name = runtimeFiberNextName();
$live->{$name} = 'ok';
echo $live->{$name};
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "set:park;set:next;ok");
    assert_eq!(
        dynamic_property_notices(&out.stderr, "RuntimeFiberGuard"),
        1,
        "{}",
        out.stderr
    );
}

/// Verifies resuming a setter restores its Fiber-local guard chain, then removes the guard when
/// the setter returns. Recreating the same receiver/name property must dispatch `__set` again.
#[test]
fn test_runtime_set_guard_survives_fiber_resume_and_unlinks_on_return() {
    let out = compile_and_run_capture(
        r#"<?php
class RuntimeResumedSetGuard {
    public function store(string $name, mixed $value): void { $this->{$name} = $value; }
    public function __set(string $name, mixed $value): void {
        echo "set:$name;";
        if ($value === 'first') {
            Fiber::suspend('paused');
            echo 'resumed;';
        }
        $this->store($name, $value);
    }
}
$object = new RuntimeResumedSetGuard();
$name = chr(112) . 'ark';
$fiber = new Fiber(function() use ($object, $name): void {
    $object->{$name} = 'first';
});
echo $fiber->start(), ';';
$fiber->resume();
echo $object->{$name}, ';';
unset($object->{$name});
$object->{$name} = 'second';
echo $object->{$name};
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "set:park;paused;resumed;first;set:park;second"
    );
}

/// Verifies `Fiber::throw()` restores the suspended guard head and the setter exception boundary
/// unlinks its node before PHP catches the delivered throwable inside the Fiber.
#[test]
fn test_runtime_set_guard_unlinks_when_fiber_throw_escapes_setter() {
    let out = compile_and_run_capture(
        r#"<?php
class RuntimeThrownSetGuard {
    public function store(string $name, mixed $value): void { $this->{$name} = $value; }
    public function __set(string $name, mixed $value): void {
        echo "set:$name;";
        if ($value === 'first') {
            Fiber::suspend('paused');
        }
        $this->store($name, $value);
    }
}
$object = new RuntimeThrownSetGuard();
$name = chr(112) . 'ark';
$fiber = new Fiber(function() use ($object, $name): void {
    try {
        $object->{$name} = 'first';
    } catch (Exception $error) {
        echo 'caught;';
    }
});
echo $fiber->start(), ';';
$fiber->throw(new Exception('delivered'));
$object->{$name} = 'second';
echo $object->{$name};
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "set:park;paused;caught;set:park;second");
}

/// Verifies native and eval property writes share one receiver/name recursion stack. Different
/// names nest through `__set`, while same-name writes cross either boundary into raw storage.
#[test]
fn test_runtime_set_guard_is_shared_across_aot_and_eval_writes() {
    let out = compile_and_run_capture(
        r#"<?php
class RuntimeEvalSetGuard {
    public function __set(string $name, mixed $value): void {
        echo "set:$name;";
        if ($name === 'outer') {
            eval('$inner = "inner"; $this->{$inner} = "nested";');
        }
        eval('$this->{$name} = $value;');
    }
}
class RuntimeEvalInitialSetGuard {
    public function __set(string $name, mixed $value): void {
        echo "initial:$name;";
        eval('$this->{$name} = $value;');
    }
}
$first = new RuntimeEvalSetGuard();
$outer = chr(111) . 'uter';
$inner = chr(105) . 'nner';
$first->{$outer} = 'value';
echo $first->{$outer}, ':', $first->{$inner}, '|';
$second = new RuntimeEvalInitialSetGuard();
$name = chr(101) . 'val';
$value = 'stored';
eval('$second->{$name} = $value;');
echo $second->{$name};
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "set:outer;set:inner;value:nested|initial:eval;stored"
    );
    assert_eq!(
        dynamic_property_notices(&out.stderr, "RuntimeEvalSetGuard"),
        2,
        "{}",
        out.stderr
    );
    assert_eq!(
        dynamic_property_notices(&out.stderr, "RuntimeEvalInitialSetGuard"),
        1,
        "{}",
        out.stderr
    );
}

/// Verifies an active name on one receiver does not suppress `__set` on another receiver.
#[test]
fn test_runtime_set_reentry_guard_includes_receiver_identity() {
    let out = compile_and_run_capture(
        r#"<?php
class RuntimePairGuard {
    public string $label;
    public ?RuntimePairGuard $peer = null;
    public function __construct(string $label) { $this->label = $label; }
    public function store(string $name, mixed $value): void { $this->{$name} = $value; }
    public function __set(string $name, mixed $value): void {
        echo $this->label, ':', $name, ';';
        if ($this->peer !== null) {
            $peer = $this->peer;
            $this->peer = null;
            $peer->{$name} = $value;
        }
        $this->store($name, $value);
    }
}
$first = new RuntimePairGuard('first');
$second = new RuntimePairGuard('second');
$first->peer = $second;
$name = chr(115) . chr(97) . chr(109) . chr(101);
$first->{$name} = 'value';
echo $first->{$name}, ':', $second->{$name};
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "first:same;second:same;value:value");
    assert_eq!(
        dynamic_property_notices(&out.stderr, "RuntimePairGuard"),
        2,
        "{}",
        out.stderr
    );
}

/// Verifies an escaping setter exception unlinks its active receiver/name guard.
#[test]
fn test_runtime_set_reentry_guard_is_removed_before_catch_resumes() {
    let out = compile_and_run_capture(
        r#"<?php
class RuntimeThrowingSet {
    public int $calls = 0;
    public function store(string $name, mixed $value): void { $this->{$name} = $value; }
    public function __set(string $name, mixed $value): void {
        $this->calls++;
        echo 'set:', $this->calls, ';';
        if ($this->calls === 1) {
            throw new Exception('first');
        }
        $this->store($name, $value);
    }
}
$object = new RuntimeThrowingSet();
$name = chr(118) . chr(97) . chr(108) . chr(117) . chr(101);
try {
    $object->{$name} = 'lost';
} catch (Exception $error) {
    echo 'caught;';
}
$object->{$name} = 'kept';
echo $object->{$name};
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "set:1;caught;set:2;kept");
    assert_eq!(
        dynamic_property_notices(&out.stderr, "RuntimeThrowingSet"),
        1,
        "{}",
        out.stderr
    );
}

/// Verifies a DIRECT-name write to a strict ancestor's private name creates a distinct dynamic
/// property, from the child scope and from global scope, and never touches the ancestor's slot.
///
/// php 7.4 removed shadow properties: `Child` has no `p` in its by-name table at all, so the write
/// is a dynamic-property creation php reports once per instance. This compiler's physical slot
/// table still carries `Base::$p` under that plain name, so the write used to be refused at
/// compile time and, once the refusal was relaxed, would have landed in `Base`'s own storage.
/// `Base::readP()` is the witness that it does not.
#[test]
fn test_direct_name_write_to_an_ancestor_private_name_creates_a_dynamic_property() {
    let out = compile_and_run_capture(
        r#"<?php
class Base {
    private $p = 'base-p';
    public function readP() { return $this->p; }
}
class Child extends Base {
    public function write($v) { $this->p = $v; }
    public function read() { return $this->p; }
}
$c = new Child();
$c->write('child-set');
echo $c->read(), "\n";
echo $c->readP(), "\n";
$c->p = 'global-set';
echo $c->p, "\n";
echo $c->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "child-set\nbase-p\nglobal-set\nbase-p\n");
    // Exactly one creation, named for the receiver's runtime class: the second write finds the
    // property already there, which php reports nothing about.
    assert_eq!(dynamic_property_notices(&out.stderr, "Child"), 1, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "Base"), 0, "{}", out.stderr);
}

/// Verifies compound assignment and pre/post increment reach the same scope-aware target.
///
/// Each of these is a read of the target followed by a write to it, so a target that disagreed
/// between the two halves would read the ancestor's slot and write the dynamic property, or the
/// reverse. The ancestor's `int` slot stays at its default throughout, which is what proves the
/// arithmetic never ran against it.
#[test]
fn test_compound_assignment_and_increment_use_the_scope_aware_target() {
    let out = compile_and_run_capture(
        r#"<?php
class Counter {
    private $n = 100;
    public function readN() { return $this->n; }
}
class Tally extends Counter {
    public function go() {
        $this->n = 1;
        $this->n += 5;
        $this->n++;
        ++$this->n;
        $this->n--;
        echo $this->n, "\n";
        echo $this->readN(), "\n";
    }
}
(new Tally())->go();
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "7\n100\n");
    assert_eq!(dynamic_property_notices(&out.stderr, "Tally"), 1, "{}", out.stderr);
}

/// Verifies `unset()` removes the distinct dynamic entry and leaves the declaring slot alone.
///
/// php's `unset($child->p)` is a no-op when nothing was created and a hash-key removal when
/// something was, and `Base::$p` survives both. The by-name ladder used to answer for the
/// ancestor's physical slot here, which would have marked THAT slot uninitialized instead.
#[test]
fn test_unset_removes_the_dynamic_entry_and_preserves_the_ancestor_slot() {
    let out = compile_and_run_capture(
        r#"<?php
class Holder {
    private $p = 'base-p';
    public function readP() { return $this->p; }
}
class User extends Holder {
    public function go() {
        unset($this->p);
        echo $this->readP(), "\n";
        $this->p = 'dyn';
        echo $this->p, "\n";
        unset($this->p);
        var_dump(isset($this->p));
        echo $this->readP(), "\n";
    }
}
(new User())->go();
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "base-p\ndyn\nbool(false)\nbase-p\n");
    assert_eq!(dynamic_property_notices(&out.stderr, "User"), 1, "{}", out.stderr);
}

/// Verifies a RUNTIME-name write reaches the same distinct dynamic property on a class that
/// carries no `#[\AllowDynamicProperties]` attribute at all.
///
/// The by-name ladder already dropped the name so it could not reach the ancestor's slot, but its
/// miss arm then found no per-instance hash, released its temporary frame and stored NOTHING: the
/// write vanished and the later read warned `Undefined property`. Reserving the hash for the
/// mutation is what gives the miss arm somewhere to write.
#[test]
fn test_runtime_name_write_to_an_ancestor_private_name_stores_without_the_attribute() {
    let out = compile_and_run_capture(
        r#"<?php
class Owner {
    private int $n = 1;
    public function readN(): int { return $this->n; }
}
class Guest extends Owner {
    public function write(string $k, int $v) { $this->$k = $v; }
    public function read(string $k) { return $this->$k; }
}
$g = new Guest();
$g->write('n', 42);
var_dump($g->read('n'));
var_dump($g->n);
echo $g->readN();
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "int(42)\nint(42)\n1");
    assert_eq!(dynamic_property_notices(&out.stderr, "Guest"), 1, "{}", out.stderr);
}

/// Verifies a RUNTIME-name `unset()` removes the distinct dynamic entry and keeps the slot.
///
/// `unset($o->{$k})` had no EIR target shape at all before this phase, for any class including
/// plain `stdClass`, so it could not compile. It now lowers to `Op::DynamicPropUnset`, whose
/// backend compares the runtime name against the receiver's declared names and then takes php's
/// answer for the matched name on the receiver's runtime class.
#[test]
fn test_runtime_name_unset_removes_the_dynamic_entry_and_preserves_the_ancestor_slot() {
    let out = compile_and_run_capture(
        r#"<?php
class Vaulted {
    private $p = 'base-p';
    public function readP() { return $this->p; }
}
class Opened extends Vaulted {
    public function write(string $k, string $v) { $this->$k = $v; }
    public function read(string $k) { return $this->$k; }
    public function drop(string $k) { unset($this->$k); }
}
$o = new Opened();
$o->write('p', 'rt');
echo $o->read('p'), "\n";
echo $o->readP(), "\n";
$o->drop('p');
var_dump(isset($o->p));
echo $o->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "rt\nbase-p\nbool(false)\nbase-p\n");
    assert_eq!(dynamic_property_notices(&out.stderr, "Opened"), 1, "{}", out.stderr);
}

/// Verifies a RUNTIME-name `unset()` removes a plain `stdClass` key.
///
/// `stdClass` keeps every property in its hash, so this is the shape with no declared name to
/// match at all: the ladder's miss arm is the whole lowering.
#[test]
fn test_runtime_name_unset_removes_a_stdclass_key() {
    let out = compile_and_run_capture(
        r#"<?php
$o = new stdClass();
$o->d = 1;
$o->e = 2;
$k = 'd';
unset($o->$k);
var_dump(isset($o->d));
var_dump($o->e);
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nint(2)\n");
}

/// Verifies a same-name private redeclaration keeps writing the SHADOW's own slot.
///
/// This is the arm that must NOT move: `Shadow` declares its own `private $p`, so the name is
/// `Visible` there and resolves to `Shadow`'s slot, not to a dynamic property and not to `Root`'s
/// slot. Two same-named private slots stay two pieces of storage.
#[test]
fn test_same_name_private_shadowing_keeps_writing_the_shadow_slot() {
    let out = compile_and_run_capture(
        r#"<?php
class Root {
    private $p = 'root-p';
    public function readRoot() { return $this->p; }
}
class Shadow extends Root {
    private $p = 'shadow-p';
    public function go() {
        $this->p = 'written';
        echo $this->p, "\n";
        echo $this->readRoot(), "\n";
    }
}
(new Shadow())->go();
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "written\nroot-p\n");
    assert!(
        !out.stderr.contains("Creation of dynamic property"),
        "a redeclared private property is an ordinary slot write: {}",
        out.stderr
    );
}

/// Verifies a boxed `Mixed` receiver writes and reads the same distinct dynamic property.
///
/// The Mixed write ladder dispatches on the receiver's runtime class id and had no arm at all for
/// a name php resolves dynamically, so the store fell through to a helper that understands
/// `stdClass` alone and was dropped. The read ladder answered a flat null for the same name, so
/// even a store that had landed could not be read back through a Mixed receiver.
#[test]
fn test_mixed_receiver_writes_and_reads_the_scope_dynamic_property() {
    let out = compile_and_run_capture(
        r#"<?php
class Secret {
    private $p = 'base-p';
    public function readP() { return $this->p; }
}
class Plain extends Secret {}
function box(): mixed { return new Plain(); }
$o = box();
$o->p = 'mixed-set';
var_dump($o->p);
echo $o->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "string(9) \"mixed-set\"\nbase-p\n");
    assert_eq!(dynamic_property_notices(&out.stderr, "Plain"), 1, "{}", out.stderr);
}

/// Verifies the dynamic-property hash is addressed at the RUNTIME class's own offset, and that
/// php's notice names the runtime class rather than the receiver's static type.
///
/// The hash pointer is an object's trailing word, so it sits at `8 + slots * 16` for the class the
/// instance actually is. A receiver's static class is only an upper bound: `Mid` declares two
/// slots and `Leaf` declares four, so addressing `Mid`'s offset on a `Leaf` wrote the hash pointer
/// straight over `Leaf`'s first own slot. That fixture SEGFAULTED before the offset was resolved
/// against the runtime subtree.
///
/// `$before` is declared before the extra slots and `$after` after them, so a wrong offset
/// corrupts one of them whichever direction it strays in, and the dynamic property, the declared
/// slots and the ancestor's private storage are all read back. The two notices pin the two runtime
/// classes by name.
#[test]
fn test_dynamic_property_hash_follows_the_runtime_class_layout() {
    let out = compile_and_run_capture(
        r#"<?php
class Anc {
    private $p = 'anc-p';
    public function readP() { return $this->p; }
}
class Mid extends Anc {
    public $before = 'mid-before';
}
class Leaf extends Mid {
    public $extra = 'leaf-extra';
    public $after = 'leaf-after';
}
function put(Mid $m, $v) { $m->p = $v; }
function get(Mid $m) { return $m->p; }
function drop(Mid $m) { unset($m->p); }
$leaf = new Leaf();
put($leaf, 'leaf-set');
echo get($leaf), "\n";
echo $leaf->before, "\n";
echo $leaf->extra, "\n";
echo $leaf->after, "\n";
echo $leaf->readP(), "\n";
drop($leaf);
var_dump(isset($leaf->p));
echo $leaf->before, "\n";
echo $leaf->after, "\n";
echo $leaf->readP(), "\n";
$mid = new Mid();
put($mid, 'mid-set');
echo get($mid), "\n";
echo $mid->before, "\n";
echo $mid->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "leaf-set\nmid-before\nleaf-extra\nleaf-after\nanc-p\nbool(false)\nmid-before\nleaf-after\nanc-p\nmid-set\nmid-before\nanc-p\n"
    );
    assert_eq!(dynamic_property_notices(&out.stderr, "Leaf"), 1, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "Mid"), 1, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "Anc"), 0, "{}", out.stderr);
}

/// Verifies a RUNTIME name on a base-typed receiver also follows the subclass layout.
///
/// Same hazard as the fixture above, reached through the runtime-name ladder instead of the
/// direct-name one: the ladder matched a name and then still had to decide WHERE that name lives
/// on the instance in hand.
#[test]
fn test_runtime_name_on_a_base_typed_receiver_follows_the_subclass_layout() {
    let out = compile_and_run_capture(
        r#"<?php
class RAnc {
    private $p = 'anc-p';
    public function readP() { return $this->p; }
}
class RMid extends RAnc {
    public $before = 'mid-before';
}
class RLeaf extends RMid {
    public $extra = 'leaf-extra';
    public $after = 'leaf-after';
}
function rput(RMid $m, string $k, string $v) { $m->$k = $v; }
function rget(RMid $m, string $k) { return $m->$k; }
$leaf = new RLeaf();
rput($leaf, 'p', 'leaf-set');
echo rget($leaf, 'p'), "\n";
echo $leaf->before, "\n";
echo $leaf->extra, "\n";
echo $leaf->after, "\n";
echo $leaf->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "leaf-set\nmid-before\nleaf-extra\nleaf-after\nanc-p\n"
    );
    assert_eq!(dynamic_property_notices(&out.stderr, "RLeaf"), 1, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "RMid"), 0, "{}", out.stderr);
}

/// Verifies a runtime subclass that redeclares the name PUBLIC uses its own slot, not the hash.
///
/// `class A { private $p; } class P extends A {} class Q extends P { public $p; }` reached through
/// a `P`-typed parameter: on a `P` the name is a dynamic property, on a `Q` it is `Q`'s own public
/// slot. Two runtime classes therefore need two different KINDS of access, which is why the
/// dispatch selects a whole action per class and not merely an offset. The absence of a notice for
/// `Q` is the assertion that carries it: a hash write would have reported one.
#[test]
fn test_runtime_subclass_redeclaring_the_name_public_uses_its_own_slot() {
    let out = compile_and_run_capture(
        r#"<?php
class SAnc {
    private $p = 'anc-p';
    public function readP() { return $this->p; }
}
class SMid extends SAnc {}
class SLeaf extends SMid {
    public $p = 'leaf-declared';
}
function sput(SMid $m, $v) { $m->p = $v; }
function sget(SMid $m) { return $m->p; }
$leaf = new SLeaf();
echo sget($leaf), "\n";
sput($leaf, 'leaf-set');
echo sget($leaf), "\n";
echo $leaf->p, "\n";
echo $leaf->readP(), "\n";
$mid = new SMid();
sput($mid, 'mid-set');
echo sget($mid), "\n";
echo $mid->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "leaf-declared\nleaf-set\nleaf-set\nanc-p\nmid-set\nanc-p\n"
    );
    assert_eq!(dynamic_property_notices(&out.stderr, "SLeaf"), 0, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "SMid"), 1, "{}", out.stderr);
}

/// Verifies the same polymorphic redeclaration through a RUNTIME name.
#[test]
fn test_runtime_name_on_a_subclass_redeclaring_the_name_public_uses_its_own_slot() {
    let out = compile_and_run_capture(
        r#"<?php
class TAnc {
    private $p = 'anc-p';
    public function readP() { return $this->p; }
}
class TMid extends TAnc {}
class TLeaf extends TMid {
    public $p = 'leaf-declared';
}
function tput(TMid $m, string $k, string $v) { $m->$k = $v; }
function tget(TMid $m, string $k) { return $m->$k; }
$leaf = new TLeaf();
echo tget($leaf, 'p'), "\n";
tput($leaf, 'p', 'leaf-set');
echo tget($leaf, 'p'), "\n";
echo $leaf->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "leaf-declared\nleaf-set\nanc-p\n");
    assert_eq!(dynamic_property_notices(&out.stderr, "TLeaf"), 0, "{}", out.stderr);
}

/// Verifies `__set`, `__get` and `__unset` intercept a strict ancestor's private name.
///
/// php consults the accessor for such a name exactly as it does for one the class never declared,
/// from the child scope and from global scope alike, and creates no dynamic property at all. The
/// three must move together: routing the write to `__set` while the read kept answering from
/// storage would report a value php never stores.
#[test]
fn test_magic_accessors_intercept_an_ancestor_private_name_from_every_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class Vault {
    private $p = 'vault-p';
    public function readP() { return $this->p; }
}
class Proxy extends Vault {
    public function __set($n, $v) { echo "set:$n\n"; }
    public function __get($n) { echo "get:$n\n"; return 'magic'; }
    public function __unset($n) { echo "unset:$n\n"; }
    public function go() { $this->p = 1; echo $this->p, "\n"; unset($this->p); }
}
$p = new Proxy();
$p->go();
echo $p->readP(), "\n";
$p->p = 2;
echo $p->p, "\n";
unset($p->p);
echo $p->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "set:p\nget:p\nmagic\nunset:p\nvault-p\nset:p\nget:p\nmagic\nunset:p\nvault-p\n"
    );
    assert!(
        !out.stderr.contains("Creation of dynamic property"),
        "an accessor answers before any property is created: {}",
        out.stderr
    );
}

/// Verifies a runtime SUBCLASS that adds an accessor its parent lacks still answers it, and that
/// the call resolves against THAT subclass rather than the receiver's static class.
///
/// The receiver is typed as the parent, which declares no accessor at all, so the decision belongs
/// to the runtime class. An `instanceof` guard peels the subclass off before the ordinary write or
/// removal, and the receiver and the value are evaluated once each, in source order, on both sides
/// of the guard. Without it a `Mixin` instance stored a dynamic property php never creates.
///
/// TWO subclasses declare the SAME accessors with DIFFERENT bodies, which is what makes this a
/// resolution test and not merely a dispatch test. Method dispatch resolves `__set` and `__unset`
/// from the receiver VALUE's static class, so a guard that branched correctly but handed the call
/// the base-typed receiver would look the accessor up on a class that declares none. Each body
/// prints its own class name, so answering from the wrong one is visible in stdout.
#[test]
fn test_runtime_subclass_accessors_answer_for_an_ancestor_private_name() {
    let out = compile_and_run_capture(
        r#"<?php
class MAnc {
    private $p = 'anc-p';
    public function readP() { return $this->p; }
}
class MMid extends MAnc {}
class Mixin extends MMid {
    public function __set($n, $v) { echo "one-set:$n=$v\n"; }
    public function __get($n) { echo "one-get:$n\n"; return 'one-magic'; }
    public function __unset($n) { echo "one-unset:$n\n"; }
}
class Mixin2 extends MMid {
    public function __set($n, $v) { echo "two-set:$n=$v\n"; }
    public function __get($n) { echo "two-get:$n\n"; return 'two-magic'; }
    public function __unset($n) { echo "two-unset:$n\n"; }
}
function mput(MMid $m, $v) { $m->p = $v; }
function mget(MMid $m) { return $m->p; }
function mdrop(MMid $m) { unset($m->p); }
$mixin = new Mixin();
mput($mixin, 'v');
echo mget($mixin), "\n";
mdrop($mixin);
echo $mixin->readP(), "\n";
$mixin2 = new Mixin2();
mput($mixin2, 'x');
echo mget($mixin2), "\n";
mdrop($mixin2);
echo $mixin2->readP(), "\n";
$plain = new MMid();
mput($plain, 'w');
echo mget($plain), "\n";
echo $plain->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "one-set:p=v\none-get:p\none-magic\none-unset:p\nanc-p\n\
         two-set:p=x\ntwo-get:p\ntwo-magic\ntwo-unset:p\nanc-p\n\
         w\nanc-p\n"
    );
    assert_eq!(dynamic_property_notices(&out.stderr, "Mixin"), 0, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "Mixin2"), 0, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "MMid"), 1, "{}", out.stderr);
}

/// Verifies an `#[\AllowDynamicProperties]` class keeps its php 8.5 exemption from the notice.
///
/// The reservation this phase adds is a STORAGE capability, never php's permission, so the notice
/// stays keyed on the attribute alone. A class that carries it stores in silence; the ordinary
/// classes above still report.
#[test]
fn test_allow_dynamic_properties_class_stores_the_ancestor_private_name_without_a_notice() {
    let out = compile_and_run_capture(
        r#"<?php
class Locked { private $p = 'locked-p'; public function readP() { return $this->p; } }
#[\AllowDynamicProperties]
class Open extends Locked {
    public function go() { $this->p = 'open-set'; echo $this->p, "\n"; echo $this->readP(), "\n"; }
}
(new Open())->go();
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "open-set\nlocked-p\n");
    assert!(
        !out.stderr.contains("Creation of dynamic property"),
        "the attribute exempts the class from php 8.5's notice: {}",
        out.stderr
    );
}

/// Verifies binding a reference to a scope-dynamic name is refused by the BACKEND.
///
/// This reaches `Op::LoadPropRefCell`, past the checker, which is why it lives here and not in the
/// error tests: `expect_error` runs `check_source` alone and can never observe a backend
/// diagnostic. php binds the reference to a distinct dynamic property in the per-instance hash,
/// which this compiler cannot alias for ANY class yet, so the answer is a refusal to compile. What
/// it must never be is the strict ancestor's slot, which is exactly what the pre-B1 reference
/// resolver handed back.
#[test]
fn test_reference_binding_to_a_scope_dynamic_name_is_refused_by_the_backend() {
    let failure = compile_source_expect_backend_error(
        r#"<?php
class RefAnc { private $p = 'anc-p'; public function readP() { return $this->p; } }
class RefChild extends RefAnc {
    public function go() { $r = &$this->p; $r = 'via-ref'; echo $this->readP(); }
}
(new RefChild())->go();
"#,
    );
    assert!(
        failure.contains("load_prop_ref_cell for dynamic or missing property RefChild::$p"),
        "expected the exact load_prop_ref_cell refusal naming RefChild::$p, got: {}",
        failure
    );
    // The ancestor's own class must never appear: naming `RefAnc` would mean the resolver had
    // walked to the declaring class and therefore to its slot, which is the escape this closes.
    assert!(
        !failure.contains("RefAnc"),
        "the refusal must not resolve through to the declaring class, got: {}",
        failure
    );
}

/// Verifies a by-reference RETURN of a scope-dynamic name is refused by the backend.
///
/// The sibling of the fixture above, through `Op::LoadPropRefCellChecked`. A by-reference return
/// hands the CALLER the address, so the refusal has to hold on that path too.
#[test]
fn test_by_reference_return_of_a_scope_dynamic_name_is_refused_by_the_backend() {
    let failure = compile_source_expect_backend_error(
        r#"<?php
class RetAnc { private $p = 'anc-p'; public function readP() { return $this->p; } }
class RetChild extends RetAnc {
    public function &ref() { return $this->p; }
}
$c = new RetChild();
$r = &$c->ref();
echo $c->readP();
"#,
    );
    assert!(
        failure
            .contains("load_prop_ref_cell_checked for dynamic or missing property RetChild::$p"),
        "expected the exact load_prop_ref_cell_checked refusal naming RetChild::$p, got: {}",
        failure
    );
    assert!(
        !failure.contains("RetAnc"),
        "the refusal must not resolve through to the declaring class, got: {}",
        failure
    );
}

/// Verifies a boxed `Mixed` receiver writes and reads a scope-dynamic name through a RUNTIME name.
///
/// The sibling of the static-name Mixed fixture above, through the other ladder. A Mixed receiver
/// with a runtime name dispatches on the runtime class id AND the name, so each arm is one
/// (class, name) pair and its hash offset and reported class name are that class's own. Without
/// the arm the store fell through to the receiver-shaped miss path, whose helper understands
/// `stdClass` alone, and the write was dropped with no diagnostic anywhere.
#[test]
fn test_mixed_receiver_runtime_name_writes_and_reads_the_scope_dynamic_property() {
    let out = compile_and_run_capture(
        r#"<?php
class MRSecret {
    private $p = 'base-p';
    public function readP() { return $this->p; }
}
class MRPlain extends MRSecret {
    public $q = 'plain-q';
}
function mrbox(): mixed { return new MRPlain(); }
$o = mrbox();
$k = 'p';
$o->$k = 'mixed-runtime-set';
var_dump($o->$k);
echo $o->readP(), "\n";
echo $o->q, "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(17) \"mixed-runtime-set\"\nbase-p\nplain-q\n"
    );
    assert_eq!(dynamic_property_notices(&out.stderr, "MRPlain"), 1, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "MRSecret"), 0, "{}", out.stderr);
}

/// Verifies a RUNTIME-name `unset()` on a boxed `Mixed` receiver removes the distinct dynamic
/// entry and leaves the ancestor's private slot and the class's own declared slot alone.
///
/// The Mixed removal ladder probes every declared (class id, name) pair first and only then each
/// class's hash, so a name a class declares clears that class's slot while a name it does not
/// declare is removed from that class's own per-instance hash at that class's own offset.
#[test]
fn test_mixed_receiver_runtime_name_unset_removes_the_dynamic_entry() {
    let out = compile_and_run_capture(
        r#"<?php
class MUSecret {
    private $p = 'base-p';
    public function readP() { return $this->p; }
}
class MUPlain extends MUSecret {
    public $q = 'plain-q';
}
function mubox(): mixed { return new MUPlain(); }
$o = mubox();
$o->p = 'mixed-set';
var_dump($o->p);
$k = 'p';
unset($o->$k);
var_dump(isset($o->p));
echo $o->readP(), "\n";
echo $o->q, "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(9) \"mixed-set\"\nbool(false)\nbase-p\nplain-q\n"
    );
    assert_eq!(dynamic_property_notices(&out.stderr, "MUPlain"), 1, "{}", out.stderr);
}

/// Verifies a `Mixed`-receiver reference binding never publishes a zero pointer as a live cell.
///
/// Both Mixed ref-cell lowerings answered a non-object receiver, and any runtime class the
/// candidate set had quietly OMITTED, by loading the immediate zero and publishing it through
/// `store_ref_cell_pointer_result`. Everything downstream treats that as a live alias and
/// dereferences it on the next read or write. A class this scope resolves dynamically is exactly
/// such an omitted class, so the safe answer is to refuse the whole lowering: php binds the
/// reference to a distinct dynamic property in the per-instance hash, which this compiler cannot
/// alias for any class yet.
#[test]
fn test_mixed_receiver_reference_binding_never_publishes_a_null_cell() {
    let failure = compile_source_expect_backend_error(
        r#"<?php
class MRefAnc {
    private $p = 'anc-p';
    public function readP() { return $this->p; }
}
class MRefKid extends MRefAnc {}
function mrefbox(): mixed { return new MRefKid(); }
$o = mrefbox();
$r = &$o->p;
$r = 'via-ref';
echo $o->readP(), "\n";
"#,
    );
    assert!(
        failure.contains(
            "for a Mixed receiver whose class MRefKid resolves $p to a dynamic property"
        ),
        "expected the exact Mixed reference refusal naming MRefKid::$p, got: {}",
        failure
    );
    assert!(
        failure.contains("load_prop_ref_cell"),
        "the refusal must name the reference-cell opcode, got: {}",
        failure
    );
}

/// Verifies the runtime-class dispatch survives probing MORE THAN ONE subclass id.
///
/// Every other layout fixture declares exactly one subclass of the parameter's static class, so
/// the ladder emits exactly one comparison and a probe that clobbers the receiver still answers
/// correctly. With two subclasses the probes run back to back, and on x86_64 the receiver lived in
/// `r11` while the candidate class id was loaded into `r11` as well: the first non-matching probe
/// overwrote the receiver with a small integer and the second probe dereferenced it. This fixture
/// reaches the second and third arms on purpose, and the two subclasses declare DIFFERENT numbers
/// of slots so a wrong arm is also a wrong hash offset.
#[test]
fn test_runtime_class_dispatch_probes_every_subclass_id() {
    let out = compile_and_run_capture(
        r#"<?php
class PolyAnc {
    private $p = 'anc-p';
    public function readP() { return $this->p; }
}
class PolyMid extends PolyAnc {
    public $a = 'mid-a';
}
class PolyOne extends PolyMid {
    public $b = 'one-b';
}
class PolyTwo extends PolyMid {
    public $c = 'two-c';
    public $d = 'two-d';
}
function polyput(PolyMid $m, $v) { $m->p = $v; }
function polyget(PolyMid $m) { return $m->p; }
$one = new PolyOne();
$two = new PolyTwo();
$mid = new PolyMid();
polyput($one, 'one-set');
polyput($two, 'two-set');
polyput($mid, 'mid-set');
echo polyget($one), "\n";
echo polyget($two), "\n";
echo polyget($mid), "\n";
echo $one->a, "\n";
echo $one->b, "\n";
echo $two->a, "\n";
echo $two->c, "\n";
echo $two->d, "\n";
echo $mid->a, "\n";
echo $one->readP(), "\n";
echo $two->readP(), "\n";
echo $mid->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "one-set\ntwo-set\nmid-set\nmid-a\none-b\nmid-a\ntwo-c\ntwo-d\nmid-a\nanc-p\nanc-p\nanc-p\n"
    );
    assert_eq!(dynamic_property_notices(&out.stderr, "PolyOne"), 1, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "PolyTwo"), 1, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "PolyMid"), 1, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "PolyAnc"), 0, "{}", out.stderr);
}

/// Verifies a RUNTIME name reaches a slot a runtime subclass INTRODUCES, on all three operations.
///
/// Every by-name ladder used to enumerate the receiver's STATIC class layout alone, which is only
/// an upper bound on the instance: `IntroKid` declares `$q` and `IntroBase` never heard of it, so
/// the name matched nothing, fell into the hash miss arm, and the write went to a per-instance
/// hash while php went to a declared slot. The two then disagreed about the value AND about where
/// it lived: `$kid->q` read the untouched slot while `$b->$k` read the hash entry.
///
/// The absence of a creation notice is what carries the assertion. A hash write on an ordinary
/// class reports one, so zero notices means every operation reached the slot. The direct read of
/// `$kid->q` is the second witness: it goes through the ordinary declared-slot path, which the
/// runtime-name write must have written for the two to agree.
#[test]
fn test_runtime_name_reaches_a_slot_a_subclass_introduces() {
    let out = compile_and_run_capture(
        r#"<?php
class IntroBase {
    private $p = 'base-p';
    public function readP() { return $this->p; }
}
class IntroKid extends IntroBase {
    public $q = 'kid-q';
}
function iput(IntroBase $b, string $k, $v) { $b->$k = $v; }
function iget(IntroBase $b, string $k) { return $b->$k; }
function idrop(IntroBase $b, string $k) { unset($b->$k); }
$kid = new IntroKid();
echo iget($kid, 'q'), "\n";
iput($kid, 'q', 'kid-set');
echo iget($kid, 'q'), "\n";
echo $kid->q, "\n";
idrop($kid, 'q');
var_dump(isset($kid->q));
echo $kid->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "kid-q\nkid-set\nkid-set\nbool(false)\nbase-p\n"
    );
    assert_eq!(dynamic_property_notices(&out.stderr, "IntroKid"), 0, "{}", out.stderr);
    assert_eq!(dynamic_property_notices(&out.stderr, "IntroBase"), 0, "{}", out.stderr);
}

/// Verifies a `readonly` class that declares `__set` answers the accessor instead of refusing.
///
/// php consults `__set` BEFORE it decides anything else about a name it does not resolve to a
/// visible slot, and a `readonly` class is no exception: the accessor answers, no dynamic property
/// is created, and the no-dynamic-properties rule has nothing to refuse. Checking the readonly
/// rule first would have rejected the program for a creation php never performs, and reserving a
/// hash for it would have charged storage no legal write can ever fill.
#[test]
fn test_readonly_class_with_a_setter_answers_the_accessor_for_an_ancestor_private_name() {
    let out = compile_and_run_capture(
        r#"<?php
readonly class ROMagicBase {
    private string $p;
    public function __construct() { $this->p = 'base-p'; }
    public function readP(): string { return $this->p; }
}
readonly class ROMagicChild extends ROMagicBase {
    public function __set($n, $v) { echo "ro-set:$n=$v\n"; }
}
$c = new ROMagicChild();
$c->p = 'x';
echo $c->readP(), "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "ro-set:p=x\nbase-p\n");
    assert_eq!(
        dynamic_property_notices(&out.stderr, "ROMagicChild"),
        0,
        "{}",
        out.stderr
    );
}

/// Verifies a runtime-name write invokes `__set` on a readonly class, while same-pair reentry
/// raises the dynamic-property `Error` instead of requiring or creating hash storage.
#[test]
fn test_readonly_runtime_name_setter_reentry_refuses_dynamic_storage() {
    let out = compile_and_run_capture(
        r#"<?php
function runtime_property_name(): string { return "x"; }
readonly class R {
    public function __set($name, $value): void {
        echo "set:$name|";
        try {
            $this->{$name} = $value;
        } catch (Error $error) {
            echo $error->getMessage();
        }
    }
}
$name = runtime_property_name();
$object = new R();
$object->{$name} = 1;
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "set:x|Cannot create dynamic property R::$x");
    assert_eq!(dynamic_property_notices(&out.stderr, "R"), 0, "{}", out.stderr);
}

/// Verifies eval can enter a native readonly `__set`, and a same-pair eval write inside that
/// setter is suppressed before the readonly class rejects dynamic storage with a catchable Error.
#[test]
fn test_readonly_runtime_name_setter_reentry_through_eval_refuses_dynamic_storage() {
    let out = compile_and_run_capture(
        r#"<?php
function runtime_eval_property_name(): string { return "x"; }
readonly class RuntimeEvalReadonlySet {
    public function __set($name, $value): void {
        echo "set:$name|";
        try {
            eval('$this->{$name} = $value;');
        } catch (Error $error) {
            echo $error->getMessage();
        }
    }
}
$name = runtime_eval_property_name();
$value = 1;
$object = new RuntimeEvalReadonlySet();
eval('$object->{$name} = $value;');
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "set:x|Cannot create dynamic property RuntimeEvalReadonlySet::$x"
    );
    assert_eq!(
        dynamic_property_notices(&out.stderr, "RuntimeEvalReadonlySet"),
        0,
        "{}",
        out.stderr
    );
}

/// Verifies opaque eval source can create and then update a dynamic property on an ordinary
/// native object. The source is returned by a runtime call so the compiler cannot pre-scan its
/// property name or lower the assignment as an AOT dynamic-property write.
#[test]
fn test_runtime_built_eval_source_round_trips_a_plain_aot_dynamic_property() {
    let out = compile_and_run_capture(
        r#"<?php
class OpaqueEvalPlain {
    public string $declared = 'kept';
}
function opaque_eval_source(string $receiver): string {
    return $receiver . '->{$name} = $value; echo $object->{$name}, "|"; '
        . '$object->{$name} = "updated"; echo $object->{$name}, "|", $object->declared;';
}
$object = new OpaqueEvalPlain();
$name = 'runtime_name';
$value = 'created';
$source = opaque_eval_source('$object');
eval($source);
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "created|updated|kept");
    assert_eq!(
        dynamic_property_notices(&out.stderr, "OpaqueEvalPlain"),
        1,
        "{}",
        out.stderr
    );
}

/// Verifies a runtime name stores on a SUBCLASS's hash when the base class has none at all.
///
/// Plan eligibility used to be decided from the static class alone: `PlainBase` has no
/// per-instance hash, so the access was not treated as dynamic and the write fell through a path
/// that dropped it, while php stores it on the `AttrKid` the receiver really is. Hash storage is a
/// per-class property, so the question has to be asked of the whole runtime subtree.
#[test]
fn test_runtime_name_stores_on_a_subclass_that_carries_the_hash() {
    let out = compile_and_run_capture(
        r#"<?php
class PlainBase {
    public $a = 'base-a';
}
#[\AllowDynamicProperties]
class AttrKid extends PlainBase {}
function aput(PlainBase $b, string $k, $v) { $b->$k = $v; }
function aget(PlainBase $b, string $k) { return $b->$k; }
$kid = new AttrKid();
aput($kid, 'dyn', 'kid-dyn');
var_dump(aget($kid, 'dyn'));
echo $kid->a, "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(out.stdout, "string(7) \"kid-dyn\"\nbase-a\n");
    assert_eq!(dynamic_property_notices(&out.stderr, "AttrKid"), 0, "{}", out.stderr);
}

/// Verifies a runtime subclass that WIDENS `protected` to `public` is reached by a runtime name.
///
/// This is the other direction of the polymorphic hazard. From global scope `ProtBase::$p` is
/// `protected` and therefore inaccessible, so a ladder that emitted the STATIC class's answer
/// raised php's access `Error` for every receiver. php asks the RUNTIME class, and on a `ProtKid`
/// the name is a public slot, so the read, the write and the `unset()` all reach that slot.
#[test]
fn test_runtime_name_uses_a_subclass_that_widens_protected_to_public() {
    let out = compile_and_run_capture(
        r#"<?php
class ProtBase {
    protected $p = 'base-p';
    public function readP() { return $this->p; }
}
class ProtKid extends ProtBase {
    public $p = 'kid-p';
}
function pput(ProtBase $b, string $k, $v) { $b->$k = $v; }
function pget(ProtBase $b, string $k) { return $b->$k; }
function pdrop(ProtBase $b, string $k) { unset($b->$k); }
$kid = new ProtKid();
echo pget($kid, 'p'), "\n";
pput($kid, 'p', 'runtime-set');
echo pget($kid, 'p'), "\n";
echo $kid->p, "\n";
echo $kid->readP(), "\n";
pdrop($kid, 'p');
var_dump(isset($kid->p));
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "kid-p\nruntime-set\nruntime-set\nruntime-set\nbool(false)\n"
    );
    assert_eq!(dynamic_property_notices(&out.stderr, "ProtKid"), 0, "{}", out.stderr);
}

/// Verifies an untyped fixed slot keeps PHP's removed state across direct, polymorphic, and Mixed
/// receiver paths. A value read warns and answers null, a probe answers false without reading the
/// released payload, and a later assignment makes the slot present again.
#[test]
fn test_untyped_fixed_slot_unset_state_is_safe_across_receiver_shapes() {
    let out = compile_and_run_capture(
        r#"<?php
class UnsetBase {
    public $direct = 'direct';
}
class UnsetKid extends UnsetBase {
    public $poly = 'poly';
    public $boxed = 'boxed';
}
function dropPoly(UnsetBase $o, string $name): void { unset($o->$name); }
function boxedKid(): mixed { return new UnsetKid(); }

$direct = new UnsetBase();
unset($direct->direct);
var_dump(isset($direct->direct), $direct->direct);
$direct->direct = 'again';
echo $direct->direct, "\n";

$poly = new UnsetKid();
dropPoly($poly, 'poly');
var_dump(isset($poly->poly), $poly->poly);
$poly->poly = 'again-poly';
echo $poly->poly, "\n";

$boxed = boxedKid();
$name = 'boxed';
unset($boxed->$name);
var_dump(isset($boxed->boxed), $boxed->boxed);
$boxed->boxed = 'again-boxed';
echo $boxed->boxed, "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nNULL\nagain\nbool(false)\nNULL\nagain-poly\nbool(false)\nNULL\nagain-boxed\n"
    );
    assert_eq!(out.stderr.matches("Warning: Undefined property:").count(), 3);
}

/// Verifies a boxed `Mixed` receiver round-trips an undeclared name through the runtime class's
/// own hash, for a RUNTIME name and for a LITERAL name alike.
///
/// Both Mixed ladders matched only DECLARED names and then fell into a miss path whose helper
/// understands `stdClass` alone, so an `#[\AllowDynamicProperties]` user class could not read back
/// what it had just stored: the write went to the class's hash and the read answered `null`, or
/// the write was dropped outright. The declared property is read alongside each one, so an arm
/// that strayed onto a slot instead of the hash is visible too.
#[test]
fn test_mixed_receiver_round_trips_an_attributed_class_hash_for_both_name_forms() {
    let out = compile_and_run_capture(
        r#"<?php
#[\AllowDynamicProperties]
class MixAttr {
    public $a = 'attr-a';
}
function mixbox(): mixed { return new MixAttr(); }
$runtime = mixbox();
$k = 'dyn';
$runtime->$k = 'runtime-dyn';
var_dump($runtime->$k);
echo $runtime->a, "\n";
$literal = mixbox();
$literal->dyn = 'literal-dyn';
var_dump($literal->dyn);
echo $literal->a, "\n";
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(11) \"runtime-dyn\"\nattr-a\nstring(11) \"literal-dyn\"\nattr-a\n"
    );
    assert_eq!(dynamic_property_notices(&out.stderr, "MixAttr"), 0, "{}", out.stderr);
}

/// Verifies a `Mixed` receiver whose runtime class declares a TYPED slot gets php's BOTH answers:
/// weak-mode coercion for a value php accepts, and a catchable `TypeError` for one it refuses.
///
/// The write ladder used to turn a value-validation failure into "no arm for this class", which
/// omitted the class entirely; the receiver then fell into the miss path, whose helper understands
/// `stdClass` alone, and the assignment VANISHED with no diagnostic anywhere. Deciding it
/// statically instead would have been wrong the other way, refusing `'5'` where php stores `5`,
/// because the verdict depends on the value's RUNTIME tag and on php's weak-mode rules. The value
/// is boxed for a runtime-shaped receiver so the existing weak-mode guard answers, and it is the
/// only thing that can.
///
/// Both halves are pinned here for that reason: a fixture with only the refusing value would pass
/// for a static refusal too, and would not notice the coercion it broke.
#[test]
fn test_mixed_receiver_typed_slot_coerces_or_refuses_like_php() {
    let out = compile_and_run_capture(
        r#"<?php
class TypedSlot {
    public int $p = 1;
}
function typedbox(): mixed { return new TypedSlot(); }
$ok = typedbox();
$ok->p = '5';
var_dump($ok->p);
$bad = typedbox();
try {
    $bad->p = 'nope';
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
var_dump($bad->p);
$runtime = typedbox();
$k = 'p';
$runtime->$k = '7';
var_dump($runtime->p);
"#,
    );
    assert!(out.success, "fixture must not fault: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "int(5)\nCannot assign string to property TypedSlot::$p of type int\nint(1)\nint(7)\n"
    );
}
