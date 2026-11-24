//! Purpose:
//! End-to-end coverage for locals whose binding may be missing on one path: a plain store after a
//! branch that returned replaces the value whatever class the branch stored, and a body checked on
//! demand from inside a loop of another body widens its own maybe-missing reads.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected outputs are php 8.5's (`php -n`); the run is under heap debug and must end clean.

use crate::support::*;

/// A local assigned one class in an `if` that returns, then another class after it, is the second class at its read, whatever statements sit between (a short ternary, a closure, a clone): the missing-binding `null` the read view carried made the second store merge into the first class (`elephc-hnb1`, Doctrine's `PersistentCollection::matching`).
#[test]
fn test_second_store_after_a_returning_branch_replaces_the_class() {
    let source = r####"<?php
interface CP { public function load(int $a, int $b): array; }
interface EP { public function load(int $a): array; }
class CPi implements CP { public function load(int $a, int $b): array { return [$a, $b]; } }
class EPi implements EP { public function load(int $a): array { return [$a]; } }
class U { public function cp(): CP { return new CPi; } public function ep(): EP { return new EPi; } }
final class A {
    public function u(): U { return new U; }
    public function plain(bool $many): array {
        if ($many) { $p = $this->u()->cp(); return $p->load(1, 2); }
        $p = $this->u()->ep();
        return $p->load(1);
    }
    public function afterShortTernary(bool $many, array $o): array {
        if ($many) { $p = $this->u()->cp(); return $p->load(1, 2); }
        $x = $o ?: [1];
        $p = $this->u()->ep();
        return $many ? [$x] : $p->load(count($x));
    }
    public function afterClosure(bool $many, array $o): array {
        if ($many) { $p = $this->u()->cp(); return $p->load(1, 2); }
        $x = $o ?: array_map(static fn (string $s): string => strtoupper($s), $o);
        $p = $this->u()->ep();
        return $p->load(count($x));
    }
    public function afterClone(int $c): array {
        if ($c > 5) { $p = $this->u()->cp(); return array_merge($p->load(1, 2), []); }
        $e = $c > 3 ? 1 : 2;
        $c = clone new DateTime("2020-01-01");
        $p = $this->u()->ep();
        return $e === 1 ? [] : array_merge($p->load($e));
    }
}
$a = new A;
echo json_encode([$a->plain(true), $a->plain(false)]), "\n";
echo json_encode([$a->afterShortTernary(true, []), $a->afterShortTernary(false, [])]), "\n";
echo json_encode([$a->afterClosure(true, []), $a->afterClosure(false, ['a'])]), "\n";
echo json_encode([$a->afterClone(9), $a->afterClone(4), $a->afterClone(1)]), "\n";"####;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "[[1,2],[1]]\n[[1,2],[1]]\n[[1,2],[1]]\n[[1,2],[],[2]]\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A method inferred on demand while another body's loop is being checked starts outside any loop: its maybe-missing read still widens with `null`, so the inferred return type is `?int` and the boxed NULL is returned as php's NULL instead of being coerced to 0.
#[test]
fn test_a_body_checked_from_inside_a_loop_widens_its_own_maybe_missing_reads() {
    let source = r####"<?php
class R {
    public function maybe(int $n) {
        if ($n > 5) { $v = 1; }
        return @$v;
    }
}
$r = new R;
for ($i = 0; $i < 8; $i += 7) {
    var_dump($r->maybe($i));
}"####;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "NULL\nint(1)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A copy of a possibly unbound local into storage that cannot widen (a reference alias, a `static`, the profiler listener's `$usageIndexReference = $usageIndexValue` in a `finally`) is accepted: the `null` the missing binding adds to the read is the warning path of the read, not a type the storage must hold (symfony's `ProfilerListener::onKernelResponse`).
#[test]
fn test_a_copy_of_a_maybe_unbound_local_into_fixed_storage_compiles() {
    let source = r####"<?php
class S { private int $u = 5; public function &getUsageIndex(): int { return $this->u; } public function get(): int { return $this->u; } }
function f(?S $session, bool $c) {
    if ($session instanceof S) {
        $v = $r = &$session->getUsageIndex();
        $r = PHP_INT_MIN;
    }
    try {
        if ($c) { return 1; }
    } finally {
        if ($session instanceof S) {
            $r = $v;
        }
    }
    return $session ? $session->get() : -1;
}
function t1(bool $c) { $r = 0; $q = &$r; if ($c) { $v = 1; } $q = $v; return $r; }
function t16(bool $c) { if ($c) { $v = 1; } static $k = 0; $k = $v; return $k; }
function t27(bool $c) { static $k = 0; $r = &$k; if ($c) { $v = 1; } $r = $v; return $k; }
$s = new S;
echo f($s, false), "\n", f($s, true), "\n", f(null, false), "\n";
echo t1(true), t16(true), t27(true), "\n";"####;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "5\n1\n-1\n111\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$b = &$z` on a name nothing assigned yet creates `$z` holding null, as php does.
#[test]
fn test_reference_to_an_unassigned_name_creates_it() {
    let source = r####"<?php
function t31() { $b = &$z; $z = 2; return [$b, $z]; }
function t32() { $b = &$z; return $b; }
echo json_encode(t31()), json_encode(t32()), "\n";"####;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[2,2]null\n");
}
