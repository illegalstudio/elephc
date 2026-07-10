//! Purpose:
//! Integration tests for PHP `clone` (EC-7 #490): shallow object copy — the PSR-7 withX()
//! immutability round-trip (mutating the clone leaves the original untouched, object-typed
//! property slots share their instance), plus `clone` staying usable as a method name and
//! the parenthesized `clone($x)` form.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Byte-parity verified against PHP 8.5 for every assertion string.

use super::*;

/// The ward-http PSR-7 pattern: `$new = clone $this; $new->prop = ...; return $new;`.
/// The original keeps its values (distinct payloads); the clone's object-typed slot shares
/// the assigned instance (shallow copy).
#[test]
fn test_clone_with_x_round_trip() {
    let out = compile_and_run(
        "<?php declare(strict_types=1); final class Inner { public function __construct(public string $tag) {} } final class Msg { private string $proto = '1.1'; private ?Inner $inner = null; public function withProto(string $p): self { $new = clone $this; $new->proto = $p; return $new; } public function withInner(Inner $i): self { $new = clone $this; $new->inner = $i; return $new; } public function proto(): string { return $this->proto; } public function innerTag(): string { return $this->inner instanceof Inner ? $this->inner->tag : 'none'; } } function main(): void { $a = new Msg(); $b = $a->withProto('2.0'); $c = $b->withInner(new Inner('x')); echo $a->proto(), ':', $b->proto(), ':', $c->proto(), ':', $a->innerTag(), ':', $c->innerTag(); } main();",
    );
    assert_eq!(out, "1.1:2.0:2.0:none:x");
}

/// `clone` stays contextual: a method literally named `clone()` still parses and dispatches,
/// `clone($x)` (parenthesized operand) clones, and a clone of a clone is a fresh copy.
#[test]
fn test_clone_keyword_stays_contextual() {
    let out = compile_and_run(
        "<?php declare(strict_types=1); final class Repo { public function clone(string $what): string { return 'cloned-' . $what; } } final class P { public function __construct(public int $v) {} } function main(): void { $r = new Repo(); $p = new P(7); $q = clone($p); $q2 = clone $q; echo $r->clone('repo'), ':', $q->v, ':', $q2->v; } main();",
    );
    assert_eq!(out, "cloned-repo:7:7");
}
