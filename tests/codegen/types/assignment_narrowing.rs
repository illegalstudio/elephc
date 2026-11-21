//! Purpose:
//! Regression coverage for issue #2307: storing a narrower value into a union-typed local
//! narrows the local's flow type, as php reads it, while the union stays its storage.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs both paths of a normalisation (`string` in, object in), so the boxed
//!   union slot is read back as the object on the path that stored it and on the one that did not.
//! - A by-reference parameter is never narrowed by a store: something outside the function's
//!   flow may write it, so the fixture shows the caller's later write through the reference.
//! - Issue #2320 fixtures cover an `if`/`elseif` chain without `else`: the fall-through holds the
//!   complement of every guard, so each of them runs every clause and the fall-through.
//! - Outputs match php 8.5 (`ELEPHC_PHP_CHECK=1`).

use super::*;

/// The issue repro: a plain store of a union member is returned as that member.
#[test]
fn test_store_narrows_union_parameter_to_the_stored_member() {
    let out = compile_and_run(
        r#"<?php
function z(\DateTimeZone|string $tz): \DateTimeZone { $tz = new \DateTimeZone('UTC'); return $tz; }
echo z('Europe/Paris')->getName(), "|", z(new \DateTimeZone('Asia/Tokyo'))->getName(), "\n";
"#,
    );
    assert_eq!(out, "UTC|UTC\n");
}

/// `if (is_string($x)) { $x = new X($x); }` leaves the object on both paths after the `if`.
#[test]
fn test_guarded_normalisation_store_joins_with_the_guard_complement() {
    let out = compile_and_run(
        r#"<?php
function z2(\DateTimeZone|string $tz): \DateTimeZone { if (\is_string($tz)) { $tz = new \DateTimeZone($tz); } return $tz; }
echo z2('Europe/Paris')->getName(), "|", z2(new \DateTimeZone('UTC'))->getName(), "\n";
"#,
    );
    assert_eq!(out, "Europe/Paris|UTC\n");
}

/// The ternary normalisation stores the narrowed ternary result back into the parameter.
#[test]
fn test_ternary_normalisation_store_narrows_union_parameter() {
    let out = compile_and_run(
        r#"<?php
function z3(\DateTimeZone|string $tz): \DateTimeZone { $tz = \is_string($tz) ? new \DateTimeZone($tz) : $tz; return $tz; }
echo z3('Europe/Paris')->getName(), "|", z3(new \DateTimeZone('UTC'))->getName(), "\n";
"#,
    );
    assert_eq!(out, "Europe/Paris|UTC\n");
}

/// A negated `instanceof` guard on a nullable interface: the store and the guard agree.
#[test]
fn test_negated_instanceof_normalisation_narrows_nullable_interface() {
    let out = compile_and_run(
        r#"<?php
function c(?\DateTimeInterface $now): \DateTimeImmutable {
    if (!$now instanceof \DateTimeImmutable) { $now = new \DateTimeImmutable('2020-01-01'); }
    return $now;
}
echo c(null)->format('Y'), "|", c(new \DateTimeImmutable('2021-01-01'))->format('Y'), "|",
    c(new \DateTime('2022-01-01'))->format('Y'), "\n";
"#,
    );
    assert_eq!(out, "2020|2021|2020\n");
}

/// The normalised parameter is stored into a typed property, as symfony/clock does.
#[test]
fn test_normalised_union_parameter_reaches_a_typed_property() {
    let out = compile_and_run(
        r#"<?php
final class Clock {
    private \DateTimeZone $tz;
    public function __construct(\DateTimeZone|string $tz) {
        if (\is_string($tz)) { $tz = new \DateTimeZone($tz); }
        $this->tz = $tz;
    }
    public function name(): string { return $this->tz->getName(); }
}
echo (new Clock('Europe/Paris'))->name(), "|", (new Clock(new \DateTimeZone('UTC')))->name(), "\n";
"#,
    );
    assert_eq!(out, "Europe/Paris|UTC\n");
}

/// Inside a loop the normalisation runs on the first iteration only; every read sees the object.
#[test]
fn test_guarded_normalisation_store_inside_a_loop() {
    let out = compile_and_run(
        r#"<?php
function zone_name(\DateTimeZone $zone): string { return $zone->getName(); }
function names(array $labels, \DateTimeZone|string $tz): string {
    $out = '';
    foreach ($labels as $label) {
        if (\is_string($tz)) { $tz = new \DateTimeZone($tz); }
        $out .= $label . '=' . zone_name($tz) . ';';
    }
    return $out;
}
echo names(['a', 'b'], 'Europe/Paris'), "|", names(['c'], new \DateTimeZone('UTC')), "\n";
"#,
    );
    assert_eq!(out, "a=Europe/Paris;b=Europe/Paris;|c=UTC;\n");
}

/// A wider member stored after the narrowing store is still accepted by the union storage.
///
/// A guard rather than a regression: it compiles on main too, and must keep doing so.
#[test]
fn test_store_after_narrowing_store_widens_back_to_the_union() {
    let out = compile_and_run(
        r#"<?php
function w(\DateTimeZone|string $tz, bool $back): string {
    $tz = new \DateTimeZone('UTC');
    if ($back) { $tz = 'Europe/Paris'; }
    return \is_string($tz) ? 'string:' . $tz : 'object';
}
echo w('x', false), "|", w('x', true), "\n";
"#,
    );
    assert_eq!(out, "object|string:Europe/Paris\n");
}

/// A by-reference parameter keeps its declared union: the caller writes it after the store.
///
/// A guard rather than a regression: narrowing `$tz` here would be unsound.
#[test]
fn test_store_does_not_narrow_a_by_reference_parameter() {
    let out = compile_and_run(
        r#"<?php
function make(bool $s): \DateTimeZone|string { return $s ? 'Europe/Paris' : new \DateTimeZone('UTC'); }
function reset_zone(\DateTimeZone|string &$tz, callable $after): string {
    $tz = new \DateTimeZone('UTC');
    $after();
    return \is_string($tz) ? 'string:' . $tz : 'object';
}
$zone = make(true);
echo reset_zone($zone, function () use (&$zone) { $zone = 'Asia/Tokyo'; }), "|";
echo reset_zone($zone, function () {}), "\n";
"#,
    );
    assert_eq!(out, "string:Asia/Tokyo|object\n");
}

/// symfony/filesystem's `FileNotFoundException::__construct`: the guarded branch stores the
/// variable on both arms of a nested `if`/`else`, so it leaves the outer `if` a string.
#[test]
fn test_nested_if_else_storing_on_both_arms_narrows_the_outer_join() {
    let out = compile_and_run(
        r#"<?php
function takes(string $m): string { return $m; }
function message(?string $message = null, ?string $path = null): string {
    if (null === $message) {
        if (null === $path) { $message = 'File could not be found.'; }
        else { $message = \sprintf('File "%s" could not be found.', $path); }
    }
    return takes($message);
}
echo message(), "|", message(null, '/tmp/a'), "|", message('given'), "\n";
"#,
    );
    assert_eq!(out, "File could not be found.|File \"/tmp/a\" could not be found.|given\n");
}

/// An `elseif` chain closed by `else`, nested deeper, and over an object union: every arm stores.
#[test]
fn test_exhaustive_nested_chains_narrow_the_outer_join() {
    let out = compile_and_run(
        r#"<?php
function takes(string $m): string { return $m; }
function label(?string $m = null, int $k = 0): string {
    if (null === $m) {
        if ($k === 1) { $m = 'one'; } elseif ($k === 2) { $m = 'two'; } else { $m = 'other'; }
    }
    return takes($m);
}
function deep(?string $m = null, int $a = 0, int $b = 0): string {
    if (null === $m) {
        if ($a) { if ($b) { $m = 'ab'; } else { $m = 'a'; } } else { $m = 'none'; }
    }
    return takes($m);
}
function zone(\DateTimeZone|string $tz, bool $named): \DateTimeZone {
    if (\is_string($tz)) {
        if ($named) { $tz = new \DateTimeZone($tz); } else { $tz = new \DateTimeZone('UTC'); }
    }
    return $tz;
}
echo label(), label(null, 1), label(null, 2), label('x'), "|", deep(), deep(null, 1), deep(null, 1, 1), "|",
    zone('Asia/Tokyo', true)->getName(), zone('Asia/Tokyo', false)->getName(),
    zone(new \DateTimeZone('Europe/Paris'), true)->getName(), "\n";
"#,
    );
    assert_eq!(out, "otheronetwox|noneaab|Asia/TokyoUTCEurope/Paris\n");
}

/// Issue #2320: an `elseif` chain without `else` joins every clause exit with the complement of
/// its guards, which is what the implicit fall-through holds.
#[test]
fn test_elseif_chain_without_else_joins_through_the_guard_complement() {
    let out = compile_and_run(
        r#"<?php
final class Tag { public function __construct(public string $name) {} }
function wrap(int|string|array $v): array {
    if (\is_int($v)) { $v = [$v]; }
    elseif (\is_string($v)) { $v = [$v]; }
    return $v;                                  // neither int nor string: already an array
}
function tag(Tag|int|string $v): Tag {
    if (\is_int($v)) { $v = new Tag('#' . $v); }
    elseif (\is_string($v)) { $v = new Tag($v); }
    return $v;
}
function listed(int|string|array $v, bool $empty): array {
    if ($empty) { $v = []; }
    elseif (\is_int($v)) { $v = [$v, $v]; }
    elseif (\is_string($v)) { return [$v, $v, $v]; }
    return $v;                                  // a diverging clause contributes no exit
}
echo count(wrap(1)), count(wrap('x')), count(wrap([1, 2])), "|",
    tag(7)->name, tag('seven')->name, tag(new Tag('given'))->name, "|",
    count(listed(1, true)), count(listed(1, false)), count(listed('x', false)), count(listed([1, 2, 3, 4], false)), "\n";
"#,
    );
    assert_eq!(out, "112|#7sevengiven|0234\n");
}

/// Issue #2320, symfony/filesystem `Filesystem::remove()`: `iterable` is `array|Traversable`, so
/// a failed `instanceof Traversable` and a failed `!is_array` both leave the array half.
#[test]
fn test_elseif_chain_narrows_iterable_through_its_array_and_traversable_halves() {
    let out = compile_and_run(
        r#"<?php
final class Fs {
    public function remove(string|iterable $files): string {
        if ($files instanceof \Traversable) { $files = iterator_to_array($files, false); }
        elseif (!\is_array($files)) { $files = [$files]; }
        return self::doRemove($files);
    }
    public function removeOther(string|iterable $files): string {
        if (\is_string($files)) { $files = [$files]; }
        elseif (!\is_array($files)) { $files = iterator_to_array($files, false); }
        return self::doRemove($files);
    }
    private static function doRemove(array $files): string { return implode(',', $files); }
}
function gen(): iterable { yield 'e'; yield 'f'; }
function passes(iterable $files): string { return (new Fs())->remove($files); }
$fs = new Fs();
echo $fs->remove('a'), "|", $fs->remove(['b', 'c']), "|", $fs->remove(new ArrayIterator(['d'])), "|",
    $fs->remove(gen()), "|", passes(['g']), "|", passes(new ArrayIterator(['h'])), "\n";
echo $fs->removeOther('a'), "|", $fs->removeOther(['b', 'c']), "|",
    $fs->removeOther(new ArrayIterator(['d'])), "|", $fs->removeOther(gen()), "\n";
"#,
    );
    assert_eq!(out, "a|b,c|d|e,f|g|h\na|b,c|d|e,f\n");
}

/// A nullable local initialized on its null arm stays non-null through the rest of the chain:
/// the later clauses and the fall-through were all entered with the guard's complement.
#[test]
fn test_lazy_nullable_local_joins_through_an_elseif_chain() {
    let out = compile_and_run(
        r#"<?php
class Service {
    public int $hits = 0;
    public function __construct(public string $name = "base") {}
    public function poke(): void { $this->hits++; }
    public function label(): string { return $this->name . ":" . $this->hits; }
}
final class Sub extends Service {}
final class Owner {
    private ?Service $service = null;
    public function get(bool $poke): Service {
        $service = $this->service;
        if ($service === null) {
            $service = new Service("lazy");
            $this->service = $service;
        } elseif ($poke) {
            $service->poke();
        }
        return $service;
    }
}
function three(?Service $s, bool $a, bool $b): Service {
    if (is_null($s)) { $s = new Sub("sub"); }
    elseif ($a) { $s->poke(); }
    elseif ($b) { $s = new Service("replaced"); }
    return $s;
}
function reguard(?Service $s): Service {
    if (!isset($s)) { $s = new Service("made"); }
    elseif ($s instanceof Sub) { $s->poke(); }
    return $s;
}
function early(?Service $s, bool $flag): Service {
    if (null === $s) { $s = new Service("made"); }
    elseif ($flag) { $s = null; return new Service("early"); }
    return $s;
}
function counter(?int $n, bool $bump): int {
    if ($n === null) { $n = 10; }
    elseif ($bump) { $n = $n + 1; }
    return $n;
}
function label(?string $s, bool $mark): string {
    if ($s === null) { $s = "none"; }
    elseif ($mark) { $s = $s . "!"; }
    return $s;
}
$owner = new Owner();
$first = $owner->get(true);
echo $first->label(), "|", $owner->get(true)->label(), "|", $owner->get(false)->label(), "|";
echo $first === $owner->get(false) ? "same" : "different", "\n";
$e = new Service("given");
echo three(null, true, true)->label(), "|", three($e, true, false)->label(), "|",
    three($e, false, true)->label(), "|", three($e, false, false)->label(), "\n";
echo reguard(null)->label(), "|", reguard(new Sub("s"))->label(), "|", reguard($e)->label(), "\n";
echo early(null, true)->label(), "|", early($e, true)->label(), "|", early($e, false)->label(), "\n";
echo counter(null, true), "|", counter(4, true), "|", counter(4, false), "|",
    label(null, true), "|", label("a", true), "|", label("a", false), "\n";
"#,
    );
    assert_eq!(
        out,
        "lazy:0|lazy:1|lazy:1|same\nsub:0|given:1|replaced:0|given:1\nmade:0|s:1|given:1\n\
         made:0|early:0|given:1\n10|5|4|none|a!|a\n",
    );
}

/// A join narrows the read, never the slot: a contract wider than the union the construct
/// started with (a kept complement, or an enclosing guard) still takes a later store.
///
/// A guard rather than a regression: both functions compile on main and must keep doing so.
#[test]
fn test_union_join_keeps_a_wider_storage_contract() {
    let out = compile_and_run(
        r#"<?php
class A {} class B {}
function flat(A|B|string $x, bool $c): string {
    if (\is_string($x)) { return 'string'; }
    if ($x instanceof A) { $x = new B(); }
    if ($c) { $x = 's'; }
    return \is_string($x) ? $x : 'object';
}
function nested(A|B|string $x, bool $c): string {
    if (!\is_string($x)) {
        if ($x instanceof A) { $x = new B(); }
        if ($c) { $x = 's'; }
    }
    return \is_string($x) ? $x : 'object';
}
function chain(A|B|int|string $x, bool $c): string {
    if (\is_string($x)) { return 'string'; }
    if ($x instanceof A) { $x = new B(); } elseif (\is_int($x)) { $x = new B(); }
    if ($c) { $x = 's'; }
    return \is_string($x) ? $x : 'object';
}
echo flat(new A(), true), flat(new A(), false), flat('q', false), "|",
    nested(new A(), true), nested(new B(), false), nested('q', true), "|",
    chain(new A(), true), chain(3, false), chain('q', false), "\n";
"#,
    );
    assert_eq!(out, "sobjectstring|sobjectq|sobjectstring\n");
}

/// A reset loop counter keeps the arithmetic `int|float` union an earlier `$i++` gave it: that
/// union models integer overflow rather than a choice of members, and the loop-storage analysis
/// of the second loop reads the counter with it.
///
/// A guard rather than a regression: it compiles on main and must keep doing so.
#[test]
fn test_store_narrowing_leaves_the_arithmetic_union_alone() {
    let out = compile_and_run(
        r#"<?php
function widen(int|float $n, bool $reset): int|float { if ($reset) { $n = 5; } return $n; }
function collect(int $n): string {
    $out = '';
    for ($i = 0; $i < $n; $i++) { $out .= $i; }
    $acc = [];
    for ($i = 0; $i < $n; $i++) { $acc = array_merge($acc, (array) $i); }
    return $out . json_encode($acc);
}
echo widen(1.5, true), widen(1.5, false), "|", collect(3), "\n";
"#,
    );
    assert_eq!(out, "51.5|012[0,1,2]\n");
}
