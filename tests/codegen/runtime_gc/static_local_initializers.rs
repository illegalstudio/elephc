//! Purpose:
//! Heap-debug regressions for function static locals: array values, and initializers with
//! observable side effects.
//!
//! Called from:
//! - `tests/codegen/runtime_gc.rs` in the codegen integration test suite.
//!
//! Key details:
//! - The initializer is evaluated only while the static is unset, as in PHP: a constructor
//!   in it runs once, not on every call. A recursive call made by the initializer can set
//!   the static first; PHP keeps that innermost value, so the outer one must be released.
//! - An append can relocate the array through `__rt_array_grow`, so the static symbol must
//!   receive the new pointer; otherwise it keeps the freed block and later calls read garbage.

use crate::support::compile_and_run_with_heap_debug;

/// Static array initializers do not leak per call, and appends survive growth.
#[test]
fn test_static_array_locals_keep_growth_and_release_later_initializers() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function remember(string $key, string $value): string {
    static $seen = ["seed" => "s"];
    $seen[$key] = $value;
    return implode(",", array_keys($seen));
}
function history(string $entry): int {
    static $log = ["start"];
    $log[] = $entry;
    return count($log);
}
function rows(): int {
    static $rows = [[0]];
    $rows[] = [count($rows)];
    return count($rows);
}
function run(): void {
    for ($i = 0; $i < 40; $i++) { remember("k" . ($i % 3), "v" . $i); history("e" . $i); rows(); }
    echo remember("z", "last"), "|", history("end"), "|", rows(), "\n";
}
run();
"#,
    );
    assert_eq!(out.stdout, "seed,k0,k1,k2,z|42|42\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "static array locals leaked: {}",
        out.stderr
    );
}

/// An object initializer runs its constructor once, and the static's destructor runs at exit.
///
/// PR #968 review: evaluating the initializer on every call printed `F,D,F,DONE,D` — a second
/// object built and destroyed on the second call — where PHP prints `F,F,DONE,D`. Recursion
/// through the initializer and a throwing initializer pin the rest of the PHP contract: the
/// innermost initialization wins, and a throw leaves the static unset for the next call.
#[test]
fn test_static_initializer_is_evaluated_only_until_the_static_is_set() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C {
    public function __construct(public string $n) {}
    public function __destruct() { echo "D\n"; }
}
function f(): string {
    static $c = new C("a");
    echo "F\n";
    return $c->n;
}
function side(string $s): string { echo "side {$s}\n"; return $s; }
function nested(int $d): string {
    static $x = $d < 2 ? nested($d + 1) . "<{$d}" : side("base");
    return $x;
}
function boom(bool $fail): string { if ($fail) { throw new Exception("boom"); } return side("ok"); }
function retried(bool $fail): string {
    static $y = boom($fail);
    return $y;
}
f();
f();
echo nested(0), "|", nested(5), "\n";
try { retried(true); } catch (Exception $e) { echo $e->getMessage(), "\n"; }
echo retried(false), "|", retried(true), "\n";
echo "DONE\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "F\nF\nside base\nbase|base\nboom\nside ok\nok|ok\nDONE\nD\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "static initializers leaked: {}",
        out.stderr
    );
}
