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
