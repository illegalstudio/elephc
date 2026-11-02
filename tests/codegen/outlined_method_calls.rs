//! Purpose:
//! Regression tests for method calls on untyped (`Mixed`/`Union`) receivers going through the
//! shared per-key helper instead of an inline class ladder at every site.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The behaviour tests compare with the output php prints; the first also demands a clean
//!   `--heap-debug` leak summary, so a helper that changed argument or result ownership fails.
//! - The assembly tests check that many sites call ONE helper per key on both targets, that
//!   by-reference shapes stay inline, and that the environment switch restores the inline ladder.

use crate::support::*;
use std::fs;

/// Arguments of several type shapes, owned temporaries, nested calls, a by-reference method
/// called through an untyped receiver (stays inline) and a `__call` fallback, with a clean leak
/// summary.
#[test]
fn test_untyped_receiver_method_calls_share_helpers_and_stay_leak_free() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public $v; function __construct($v) { $this->v = $v; } }
class A { public function run($a, $b) { return $a . '-' . $b . 'A'; } public function one($x) { return $x + 1; } public function arr($x) { return [$x, $x]; } public function make($x) { return new Box($x); } }
class B { public function run($a, $b) { return $a . $b . 'B'; } public function one($x) { return $x * 2; } public function arr($x) { return ['k' => $x]; } public function make($x) { return null; } }
class C { public function run($a, $b = 'dflt') { return strtoupper($a) . $b; } public function one(int $x) { return $x - 1; } public function arr($x) { return []; } public function make($x) { return new A; } }
class D extends A { public function run($a, $b) { return 'D' . parent::run($a, $b); } }
class M { public function __call($n, $a) { return "magic:$n:" . count($a); } }
class T { public function byref(&$x) { $x++; return $x; } public function run($a, $b) { return 'T'; } public function one($x) { return 't'; } }
class U { public function byref(&$x) { $x += 10; return $x; } public function run($a, $b) { return 'U'; } public function one($x) { return 'u'; } }
class V { public function byref(&$x) { $x .= 'v'; return $x; } public function run($a, $b) { return 'V'; } public function one($x) { return 'v'; } }
function run($o, $a, $b) { return $o->run($a, $b); }
function one($o, $x) { return $o->one($x); }
function arr($o, $x) { return $o->arr($x); }
function make($o, $x) { return $o->make($x); }
function byref($o, $x) { $o->byref($x); return $x; }
function chain($o, $x) { return $o->run($o->run($x, 'p'), $o->run('q', $x)); }
function main() {
$objs = [new A, new B, new C, new D];
$acc = '';
for ($i = 0; $i < 200; $i++) {
    foreach ($objs as $o) {
        $acc .= run($o, 'x' . $i, 'y' . ($i % 5)) . '|';
        $acc .= json_encode(arr($o, "v$i")) . '|';
        $r = make($o, $i);
        $acc .= (is_object($r) ? get_class($r) : var_export($r, true)) . '|';
        $acc .= var_export(one($o, $i % 2 ? "$i" : $i), true) . '|';
        $acc .= chain($o, "c$i") . '|';
    }
}
echo strlen($acc), ':', md5($acc), "\n";
echo run(new M, 1, 2), "\n";
$z = 5; echo byref(new T, $z), byref(new U, $z), byref(new V, $z), "\n";

}
main();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "32748:912b2a728778a16c315c1e8ca02b71f2\nmagic:run:2\n6155v\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Exceptions thrown inside the method, a missing method, a non-object receiver and a null
/// receiver behave as php prints them, and the exception's line and trace depth are the call's.
#[test]
fn test_untyped_receiver_method_call_errors_and_exceptions_match_php() {
    let out = compile_and_run(
        r#"<?php
class Box { public $v; function __construct($v) { $this->v = $v; } }
class A { public function run($a, $b) { return $a . '-' . $b . 'A'; } public function one($x) { return $x + 1; } public function arr($x) { return [$x, $x]; } public function make($x) { return new Box($x); } public function boom($x) { throw new RuntimeException("A:$x"); } }
class B { public function run($a, $b) { return $a . $b . 'B'; } public function one($x) { return $x * 2; } public function arr($x) { return ['k' => $x]; } public function make($x) { return null; } public function boom($x) { return $x; } }
class C { public function run($a, $b = 'dflt') { return strtoupper($a) . $b; } public function one(int $x) { return $x - 1; } public function arr($x) { return []; } public function make($x) { return new A; } public function boom($x) { return $x; } }
class D extends A { public function run($a, $b) { return 'D' . parent::run($a, $b); } }
class M { public function __call($n, $a) { return "magic:$n:" . count($a); } }
class T { public function byref(&$x) { $x++; return $x; } public function run($a, $b) { return 'T'; } public function one($x) { return 't'; } }
class U { public function byref(&$x) { $x += 10; return $x; } public function run($a, $b) { return 'U'; } public function one($x) { return 'u'; } }
class V { public function byref(&$x) { $x .= 'v'; return $x; } public function run($a, $b) { return 'V'; } public function one($x) { return 'v'; } }
function run($o, $a, $b) { return $o->run($a, $b); }
function run1($o, $a) { return $o->run($a); }
function one($o, $x) { return $o->one($x); }
function arr($o, $x) { return $o->arr($x); }
function make($o, $x) { return $o->make($x); }
function boom($o, $x) { return $o->boom($x); }
function byref($o, $x) { $o->byref($x); return $x; }
function chain($o, $x) { return $o->run($o->run($x, 'p'), $o->run('q', $x)); }
$objs = [new A, new B, new C, new D];
$acc = '';
for ($i = 0; $i < 200; $i++) {
    foreach ($objs as $o) {
        $acc .= run($o, 'x' . $i, 'y' . ($i % 5)) . '|';
        $acc .= json_encode(arr($o, "v$i")) . '|';
        $r = make($o, $i);
        $acc .= (is_object($r) ? get_class($r) : var_export($r, true)) . '|';
        $acc .= var_export(one($o, $i % 2 ? "$i" : $i), true) . '|';
        $acc .= chain($o, "c$i") . '|';
        try { $acc .= boom($o, $i) . '|'; } catch (RuntimeException $e) { $acc .= $e->getMessage() . '|'; }
    }
}
echo strlen($acc), ':', md5($acc), "\n";
echo run1(new C, 'z'), "\n";
echo run(new M, 1, 2), "\n";
try { run(new stdClass, 1, 2); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { run(null, 1, 2); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$z = 5; echo byref(new T, $z), byref(new U, $z), byref(new V, $z), "\n";
try { boom(new A, 'x'); } catch (RuntimeException $e) { echo $e->getMessage(), ' ', $e->getLine(), ' ', count($e->getTrace()), "\n"; }
"#,
    );
    assert_eq!(out, "36308:a878339d8365b33222f1665a8735cc10\nZdflt\nmagic:run:2\nError: Call to undefined method stdClass::run()\nError: Call to a member function run() on null\n6155v\nA:x 3 2\n");
}

/// Compiles `source` with `--emit-asm` for `target`, with extra environment, and returns the
/// assembly text.
fn emit_asm_with_env(prefix: &str, source: &str, target: &str, env: &[(&str, &str)]) -> String {
    let dir = make_cli_test_dir(prefix);
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command
        .arg("--target")
        .arg(target)
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --emit-asm");
    assert!(
        output.status.success(),
        "elephc --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read emitted assembly");
    let _ = fs::remove_dir_all(&dir);
    asm
}

/// Counts the assembly lines that define a label starting with `prefix`.
fn count_definitions(asm: &str, prefix: &str) -> usize {
    asm.lines()
        .filter(|line| line.starts_with(prefix) && line.trim_end().ends_with("_0:"))
        .count()
}

/// Counts the assembly lines that call a label starting with `prefix`.
fn count_calls(asm: &str, prefix: &str) -> usize {
    asm.lines()
        .filter(|line| {
            let line = line.trim();
            (line.starts_with("bl ") || line.starts_with("call ")) && line.contains(prefix)
        })
        .count()
}

/// Four classes declare `m`/`k`/`bump`; six sites call `m` with the same operand shapes, two call
/// `k`, and `bump` takes its argument by reference.
const MANY_SITES: &str = r#"<?php
class One { public function m($a, $b) { return $a . $b . "1"; } public function k($x) { return $x + 1; } public function bump(&$x) { $x++; } }
class Two { public function m($a, $b) { return $a . $b . "2"; } public function k($x) { return $x + 2; } public function bump(&$x) { $x += 2; } }
class Three { public function m($a, $b) { return $a . $b . "3"; } public function k($x) { return $x + 3; } public function bump(&$x) { $x += 3; } }
class Four { public function m($a, $b) { return $a . $b . "4"; } public function k($x) { return $x + 4; } public function bump(&$x) { $x += 4; } }
function m1($o, $x) { return $o->m("a", $x); }
function m2($o, $x) { return $o->m("b", $x); }
function m3($o, $x) { return $o->m("c", $x); }
function m4($o, $x) { return $o->m("d", $x); }
function m5($o, $x) { return $o->m("e", $x); }
function m6($o, $x) { return $o->m("f", $x); }
function k1($o) { return $o->k(1); }
function k2($o) { return $o->k(2); }
function bump($o, $x) { $o->bump($x); return $x; }
foreach ([new One, new Two, new Three, new Four] as $o) {
    echo m1($o, 1), m2($o, 2), m3($o, 3), m4($o, 4), m5($o, 5), m6($o, 6), k1($o), k2($o), bump($o, 10), "\n";
}
"#;

/// Every site with the same key calls the one helper, on both targets.
#[test]
fn test_untyped_receiver_method_call_sites_share_one_helper_per_key() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let asm = emit_asm_with_env("elephc_method_call_shared", MANY_SITES, target, &[]);
        let prefix = "_eir_shared_mixed_method_call_";
        let defs = count_definitions(&asm, prefix);
        let calls = count_calls(&asm, prefix);
        // `m` (string, mixed) and `k` (int): two keys, whatever the number of sites.
        assert_eq!(defs, 2, "{target}: expected one helper per key, found {defs}");
        assert_eq!(calls, 8, "{target}: expected every `m`/`k` site to call a helper, found {calls}");
    }
}

/// Three `ArrayAccess`/`Countable` classes answered through `int`, `bool` and `float` results:
/// `isset($a[$k])` dispatching `offsetExists()`, `$a->count()`, `$a->ratio()`.
const SCALAR_SITES: &str = r#"<?php
class Bag implements ArrayAccess, Countable {
    public function __construct(private array $d = []) {}
    public function offsetExists(mixed $k): bool { return isset($this->d[$k]); }
    public function offsetGet(mixed $k): mixed { return $this->d[$k] ?? null; }
    public function offsetSet(mixed $k, mixed $v): void { $this->d[$k] = $v; }
    public function offsetUnset(mixed $k): void { unset($this->d[$k]); }
    public function count(): int { return count($this->d); }
    public function ratio(): float { return count($this->d) / 4; }
}
class Box implements ArrayAccess, Countable {
    public function offsetExists(mixed $k): bool { return $k === 'box'; }
    public function offsetGet(mixed $k): mixed { return 'B'; }
    public function offsetSet(mixed $k, mixed $v): void {}
    public function offsetUnset(mixed $k): void {}
    public function count(): int { return 7; }
    public function ratio(): float { return 0.5; }
}
class Tri { public function count(): int { return 3; } public function ratio(): float { return 1.5; } public function offsetExists($k): bool { return false; } }
function has(mixed $a, string $k): bool { return isset($a[$k]); }
function cnt(mixed $a): int { return $a->count(); }
function cnt2(mixed $a): int { return $a->count(); }
function rt(mixed $a): float { return $a->ratio(); }
function same(mixed $a, string $k): string { return var_export($a->offsetExists($k), true); }
$items = $argc > 5 ? [] : [new Bag(['x' => 1, 'y' => null]), new Box(), new Tri()];
foreach ($items as $i) {
    if (!($i instanceof Tri)) { echo var_export(has($i, 'x'), true), var_export(has($i, 'y'), true), var_export(has($i, 'box'), true), ' '; }
    echo same($i, 'box'), ' ', cnt($i), ' ', cnt2($i), ' ', rt($i), "\n";
}
parse_str("a[b][c]=1&a[b][d]=2&x=3&a[]=4", $out);
echo json_encode($out), "\n";
"#;

/// A call whose result is a plain `int`, `bool` or `float` goes through the shared helper too.
/// The `parse_str()` prelude asks `offsetExists()` of an untyped container and gets a `bool`:
/// that site kept its whole `ArrayAccess` class ladder (892 such sites, part of the 5 MB of inline
/// method ladders in the compiled Symfony console). The answers match php with a clean heap.
#[test]
fn test_untyped_receiver_scalar_result_calls_share_helpers() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let asm = emit_asm_with_env("elephc_method_call_scalar", SCALAR_SITES, target, &[]);
        assert!(
            !asm.lines().any(|line| line.contains("_parse_str_inser_mixed_method_")),
            "{target}: the parse_str() prelude kept its inline offsetExists() ladder"
        );
        assert!(
            count_calls(&asm, "_eir_shared_mixed_method_call_") > 0,
            "{target}: no site calls a shared method-call helper"
        );
    }
    let out = compile_and_run_with_heap_debug(SCALAR_SITES);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "truefalsefalse false 2 2 0.5\nfalsefalsetrue true 7 7 0.5\nfalse 3 3 1.5\n{\"a\":{\"b\":{\"c\":\"1\",\"d\":\"2\"},\"0\":\"4\"},\"x\":\"3\"}\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A by-reference method cannot be reached from a helper, so its site keeps the inline ladder;
/// the switch brings every site back to an inline ladder.
#[test]
fn test_untyped_receiver_method_call_inline_fallbacks() {
    let asm = emit_asm_with_env("elephc_method_call_inline", MANY_SITES, "macos-aarch64", &[]);
    let by_ref_arms = asm
        .lines()
        .filter(|line| line.contains("_bump_mixed_method_One_") && line.trim_end().ends_with(':'))
        .count();
    assert!(by_ref_arms >= 1, "the by-reference site must keep its inline ladder");
    let off = emit_asm_with_env(
        "elephc_method_call_off",
        MANY_SITES,
        "macos-aarch64",
        &[("ELEPHC_NO_OUTLINED_METHOD_CALLS", "1")],
    );
    assert_eq!(count_definitions(&off, "_eir_shared_mixed_method_call_"), 0);
    assert_eq!(count_calls(&off, "_eir_shared_mixed_method_call_"), 0);
}
