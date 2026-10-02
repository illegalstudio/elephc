//! Purpose:
//! Tests nullable object and callable coalescing ownership, including non-null value arms.
//!
//! Called from:
//! - The runtime_gc codegen integration suite through Rust's test harness.
//!
//! Key details:
//! - Distinct non-null property and ternary inputs must not select the fallback object.
//! - Repeated native execution must finish with a clean heap.

use crate::support::compile_and_run_with_heap_debug;

/// Coalescing a nullable object with `new` (`$o ??= new Box()`, `$o = $o ?? new Box()`) leaves
/// the object itself in `$o`, not the boxed cell that held `?Box`: every member access after it
/// read the cell as the object and crashed or answered garbage (#1628). The unbox is taken by a
/// `?Box` parameter both null and not, a nullable call result and a nullable property (`a`-`d`),
/// and by a nullable callable coalesced with a closure (`h`). `$o ?? null` stays nullable (`g`).
/// A local merged from a ternary (`e`) and `?:` (`f`) keep a `Mixed` temp and pin the unchanged
/// neighbouring paths. Runs over a loop under `--heap-debug`.
/// Non-null property and ternary inputs contain `n = 11`, distinct from the fallback's `7`;
/// the ternary selector reads `$argc` so both arms remain materialized at runtime.
#[test]
fn test_null_coalesce_nullable_object_with_new_keeps_the_object() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box {
    public array $list = [1, 2, 3];
    public int $n = 7;
    public ?Box $next = null;
    public function items(): array { return $this->list; }
}
function find(int $n): ?Box { return $n > 0 ? new Box() : null; }
function a(?Box $o): int { $o ??= new Box(); return $o->n + count($o->items()); }
function b(?Box $o): int { $o = $o ?? new Box(); return $o->list[2]; }
function c(int $n): int { $o = find($n) ?? new Box(); return $o->n; }
function d(Box $h): int { $x = $h->next ?? new Box(); return $x->n; }
function e(?Box $o, int $n): int { $x = $n > 0 ? $o : new Box(); $x ??= new Box(); return $x->n; }
function f(?Box $o): int { $x = $o ?: new Box(); return $x->n; }
function g(?Box $o): string { $x = $o ?? null; return $x === null ? "null" : (string) $x->n; }
function h(?callable $cb): string { $cb2 = $cb ?? fn() => "dflt"; return $cb2(); }
$nonnull = new Box();
$nonnull->n = 11;
$holder = new Box();
$holder->next = $nonnull;
$total = 0;
$tail = "";
for ($i = 0; $i < 20 + ($argc > 5 ? 1 : 0); $i++) {
    $total += a(null) + a(new Box()) + b(null) + b(new Box()) + c(0) + c(1) + d(new Box()) + d($holder) + e(null, 1) + e(new Box(), 0) + e($nonnull, $argc) + f(null) + f(new Box());
    $tail = g(null) . g(new Box()) . h(null) . h(fn() => "cb");
}
echo $total, " ", $tail, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1940 null7dfltcb\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
